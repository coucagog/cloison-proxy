//! Client du sidecar `cloison-detect` — wiring edge→detect (B.1).
//!
//! Le proxy appelle le sidecar NER (REST `POST /detect`, repli du contrat
//! gRPC nominal) **en plus** de sa détection embarquée : les spans PERSON/LOC
//! du sidecar (Presidio/GLiNER/afroxlmr — le fossé ouest-africain) sont
//! fusionnés par `cloison-core` après validation stricte
//! (`Engine::tokenize_with_extra` — le core reste la source de vérité).
//!
//! Dégradation gracieuse : toute erreur du sidecar (indisponible, timeout,
//! réponse invalide) est loguée en `warn` et le proxy continue avec la
//! détection embarquée seule — **jamais de crash, jamais de blocage de la
//! requête** (cohérent avec le design du sidecar, "jamais de crash").
//!
//! Le contrat REST (miroir du proto) :
//!   POST /detect  { text, locale, policy?, session?, core_spans[] }
//!   →             { spans: [{start, end, type, score}], quasi_id? }
//! Seuls `PERSON` et `LOC` sont retenus (les autres types NER sont ignorés ;
//! MAIL/TEL/CNI restent du ressort déterministe du core).

use std::time::Duration;

use cloison_core::{DetectorKind, Span};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::DetectConfig;
use crate::errors::{ErrorKind, ProxyError};

/// Client HTTP du sidecar detect (stateless, réutilisable).
#[derive(Clone)]
pub struct DetectClient {
    http: reqwest::Client,
    url: Url,
    timeout: Duration,
}

impl std::fmt::Debug for DetectClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetectClient")
            .field("url", &self.url)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl DetectClient {
    /// Construit le client depuis la configuration detect.
    pub fn new(config: &DetectConfig) -> Result<Self, ProxyError> {
        let url = config
            .url
            .clone()
            .ok_or_else(|| ProxyError::new(ErrorKind::Internal, "detect client without url"))?;
        let http = reqwest::Client::builder()
            .connect_timeout(config.timeout)
            .build()
            .map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to build detect http client")
                    .with_field("detail", e.to_string())
            })?;
        Ok(Self {
            http,
            url,
            timeout: config.timeout,
        })
    }

    /// Appelle le sidecar sur `text` et renvoie les spans NER **validés côté
    /// format** (offsets + type) ; la valeur est laissée vide — le core la
    /// re-tranche du texte (jamais de confiance aveugle au sidecar).
    pub async fn detect(&self, text: &str) -> Result<Vec<Span>, ProxyError> {
        let body = DetectRequestBody {
            text,
            locale: "fr",
            policy: None,
            session: None,
            core_spans: &[],
        };
        let resp = self
            .http
            .post(self.url.clone())
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                ProxyError::new(ErrorKind::Upstream, "detect sidecar request failed")
                    .with_field("detail", e.to_string())
            })?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(
                ProxyError::new(ErrorKind::Upstream, "detect sidecar error status")
                    .with_field("status", status.to_string())
                    .with_field("detail", detail),
            );
        }
        let parsed: DetectResponseBody = resp.json().await.map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid detect sidecar response")
                .with_field("detail", e.to_string())
        })?;
        let mut spans = Vec::new();
        for s in parsed.spans {
            let kind = match s.type_.as_str() {
                "PERSON" => DetectorKind::Person,
                "LOC" => DetectorKind::Location,
                // ORG et autres types NER : hors périmètre core actuel —
                // ignorés (MAIL/TEL/CNI sont déterministes, côté core).
                _ => continue,
            };
            // Bornes cohérentes (le core re-valide contre le texte).
            if s.start >= s.end {
                continue;
            }
            spans.push(Span {
                entity_type: kind,
                start: s.start,
                end: s.end,
                score: s.score,
                value: String::new(),
            });
        }
        Ok(spans)
    }
}

/// Corps de requête REST — miroir du proto (champs camelCase).
#[derive(Serialize)]
struct DetectRequestBody<'a> {
    text: &'a str,
    locale: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<DetectPolicyRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<serde_json::Value>,
    core_spans: &'a [CoreSpanRequest],
}

/// Politique minimale transmise au sidecar : `None` = défauts du sidecar.
#[derive(Serialize)]
struct DetectPolicyRequest {
    types: Vec<String>,
    min_score: f64,
}

/// Span embarqué communiqué au sidecar (désambiguïsation). Vide ici : le core
/// déduplique lui-même les chevauchements avec ses spans.
#[derive(Serialize)]
struct CoreSpanRequest {
    start: usize,
    end: usize,
    #[serde(rename = "type")]
    type_: &'static str,
    score: f64,
}

/// Corps de réponse REST — miroir du proto.
#[derive(Deserialize)]
struct DetectResponseBody {
    spans: Vec<RestSpan>,
}

#[derive(Deserialize)]
struct RestSpan {
    start: usize,
    end: usize,
    #[serde(rename = "type")]
    type_: String,
    score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mapping des types NER du sidecar → DetectorKind du core.
    fn map_type(t: &str) -> Option<DetectorKind> {
        match t {
            "PERSON" => Some(DetectorKind::Person),
            "LOC" => Some(DetectorKind::Location),
            _ => None,
        }
    }

    #[test]
    fn test_sidecar_type_mapping() {
        assert_eq!(map_type("PERSON"), Some(DetectorKind::Person));
        assert_eq!(map_type("LOC"), Some(DetectorKind::Location));
        assert_eq!(map_type("ORG"), None);
        assert_eq!(map_type("MAIL"), None);
        assert_eq!(map_type(""), None);
    }
}
