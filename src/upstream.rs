//! Client amont (reqwest) : injection de la clé amont dans le header
//! `Authorization` uniquement — jamais en URL, jamais dans le corps, jamais
//! dans un log (invariant I1).

use url::Url;
use zeroize::Zeroizing;

use crate::config::UpstreamConfig;
use crate::errors::{ErrorKind, ProxyError};

/// Client HTTP vers le fournisseur LLM.
pub struct UpstreamClient {
    http: reqwest::Client,
    /// Configuration amont (lue par le routeur pour la limite de corps).
    pub config: UpstreamConfig,
}

impl UpstreamClient {
    /// Construit le client (timeouts connect/request).
    pub fn new(config: &UpstreamConfig) -> Result<Self, ProxyError> {
        // PAS de timeout de corps global : un timeout sur l'ensemble du corps
        // couperait les streams SSE longs (violation fail-loud silencieuse).
        // Seuls connect et read sont bornes ; le stream gere lui-meme son idle.
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.request_timeout)
            .build()
            .map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to build upstream HTTP client")
                    .with_field("detail", crate::errors::truncate(&e.to_string(), 512))
            })?;
        Ok(Self {
            http,
            config: config.clone(),
        })
    }

    /// Construit l'URL à partir de `base_url` + chemin — **jamais** de clé en
    /// query string ni en chemin.
    fn url(&self, path: &str) -> Result<Url, ProxyError> {
        self.config
            .base_url
            .join(path)
            .map_err(|e| ProxyError::new(ErrorKind::Internal, "invalid upstream URL").with_field("detail", e.to_string()))
    }

    /// Non-stream `chat/completions` : envoie le corps (déjà tokenisé) avec
    /// `Authorization: Bearer <cle_amont>`.
    pub async fn chat_completions(
        &self,
        upstream_key: &Zeroizing<String>,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ProxyError> {
        let resp = self
            .http
            .post(self.url(&self.config.chat_completions_path)?)
            .bearer_auth(upstream_key.as_str())
            .json(&body)
            .send()
            .await?;
        check_success(resp).await
    }

    /// Stream `chat/completions` : retourne la réponse HTTP brute (corps lu
    /// ensuite par `stream::sse_response`). Statut non 2xx → 502 avant tout
    /// octet SSE.
    pub async fn chat_completions_stream(
        &self,
        upstream_key: &Zeroizing<String>,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ProxyError> {
        let resp = self
            .http
            .post(self.url(&self.config.chat_completions_path)?)
            .bearer_auth(upstream_key.as_str())
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            let status = resp.status();
            // Le corps amont est tronqué (limite de logs) et ne contient de
            // toute façon pas la clé (elle n'a jamais quitté le header).
            let body_text = resp.text().await.unwrap_or_default();
            tracing::warn!(
                status = %status.as_u16(),
                body = %crate::errors::truncate(&body_text, 1024),
                "upstream non-2xx on stream request"
            );
            Err(ProxyError::new(ErrorKind::Upstream, "upstream returned an error status")
                .with_field("status", status.as_u16().to_string()))
        }
    }

    /// Non-stream `completions` (legacy).
    pub async fn completions(
        &self,
        upstream_key: &Zeroizing<String>,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ProxyError> {
        let resp = self
            .http
            .post(self.url(&self.config.completions_path)?)
            .bearer_auth(upstream_key.as_str())
            .json(&body)
            .send()
            .await?;
        check_success(resp).await
    }

    /// `GET /v1/models` — pass-through (aucune tokenisation).
    pub async fn models(&self, upstream_key: &Zeroizing<String>) -> Result<serde_json::Value, ProxyError> {
        let resp = self
            .http
            .get(self.url(&self.config.models_path)?)
            .bearer_auth(upstream_key.as_str())
            .send()
            .await?;
        check_success(resp).await
    }
}

/// Vérifie le statut ; parse le JSON du corps. Statut non 2xx → 502 (le corps
/// amont, tronqué, ne va que dans les logs).
async fn check_success(resp: reqwest::Response) -> Result<serde_json::Value, ProxyError> {
    let status = resp.status();
    if status.is_success() {
        resp.json().await.map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid JSON from upstream")
                .with_field("detail", crate::errors::truncate(&e.to_string(), 512))
        })
    } else {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            status = %status.as_u16(),
            body = %crate::errors::truncate(&body, 1024),
            "upstream non-2xx response"
        );
        Err(ProxyError::new(ErrorKind::Upstream, "upstream returned an error status")
            .with_field("status", status.as_u16().to_string()))
    }
}
