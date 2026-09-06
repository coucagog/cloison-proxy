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
    ///
    /// Gère les deux conventions :
    ///   base = https://openrouter.ai            + /v1/chat/completions
    ///   base = https://openrouter.ai/api/v1     + /chat/completions
    /// et évite le doublon de préfixe API :
    ///   base = https://openrouter.ai/api/v1     + /v1/chat/completions
    ///   -> https://openrouter.ai/api/v1/chat/completions
    fn url(&self, path: &str) -> Result<Url, ProxyError> {
        let base = self.config.base_url.as_str().trim_end_matches('/');
        let mut full = format!("{base}{path}");
        // Anti-doublon : si la base se termine par /v1 (ou /api/v1) et que le
        // chemin commence par /v1/, retirer le /v1 du chemin.
        if (base.ends_with("/v1") || base.ends_with("/api/v1")) && path.starts_with("/v1/") {
            full = format!("{base}{}", &path["/v1".len()..]);
        }
        Url::parse(&full).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid upstream URL")
                .with_field("detail", e.to_string())
        })
    }

    /// Non-stream `chat/completions` : envoie le corps (déjà tokenisé) avec
    /// `Authorization: Bearer <cle_amont>`.
    ///
    /// Dégradation ciblée : un 400 « response_format type is unavailable »
    /// (DeepSeek direct) déclenche UN réessai sans `response_format` — le
    /// client évite la triple latence de ses propres retries. Tout autre 400
    /// reste un 502 (jamais de retry aveugle).
    pub async fn chat_completions(
        &self,
        upstream_key: &Zeroizing<String>,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ProxyError> {
        let url = self.url(&self.config.chat_completions_path)?;
        let resp = self
            .http
            .post(url.clone())
            .bearer_auth(upstream_key.as_str())
            .json(&body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if response_format_400(status, &body, &text) {
            tracing::warn!("amont refuse response_format (400) — réessai dégradé sans response_format");
            let stripped = strip_response_format(&body);
            let resp = self
                .http
                .post(url)
                .bearer_auth(upstream_key.as_str())
                .json(&stripped)
                .send()
                .await?;
            return check_success(resp).await;
        }
        result_from_parts(status, &text)
    }

    /// Stream `chat/completions` : retourne la réponse HTTP brute (corps lu
    /// ensuite par `stream::sse_response`). Statut non 2xx → 502 avant tout
    /// octet SSE. Même dégradation `response_format` que le non-stream.
    pub async fn chat_completions_stream(
        &self,
        upstream_key: &Zeroizing<String>,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ProxyError> {
        let url = self.url(&self.config.chat_completions_path)?;
        let resp = self
            .http
            .post(url.clone())
            .bearer_auth(upstream_key.as_str())
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body_text = resp.text().await.unwrap_or_default();
        if response_format_400(status, &body, &body_text) {
            tracing::warn!("amont refuse response_format (400) — réessai dégradé sans response_format (stream)");
            let stripped = strip_response_format(&body);
            let resp = self
                .http
                .post(url)
                .bearer_auth(upstream_key.as_str())
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(&stripped)
                .send()
                .await?;
            if resp.status().is_success() {
                return Ok(resp);
            }
            return stream_error(resp).await;
        }
        stream_error_from_parts(status, &body_text)
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
    pub async fn models(
        &self,
        upstream_key: &Zeroizing<String>,
    ) -> Result<serde_json::Value, ProxyError> {
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
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    result_from_parts(status, &text)
}

/// Statut + corps déjà lus → résultat (JSON si 2xx, 502 sinon).
fn result_from_parts(status: u16, text: &str) -> Result<serde_json::Value, ProxyError> {
    if (200..300).contains(&status) {
        serde_json::from_str(text).map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid JSON from upstream")
                .with_field("detail", crate::errors::truncate(&e.to_string(), 512))
        })
    } else {
        tracing::warn!(
            status = %status,
            body = %crate::errors::truncate(text, 1024),
            "upstream non-2xx response"
        );
        Err(ProxyError::new(ErrorKind::Upstream, "upstream returned an error status")
            .with_field("status", status.to_string()))
    }
}

/// Erreur stream (statut non 2xx, après retry éventuel) — même contrat.
async fn stream_error(resp: reqwest::Response) -> Result<reqwest::Response, ProxyError> {
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    stream_error_from_parts(status, &text)
}

fn stream_error_from_parts(status: u16, text: &str) -> Result<reqwest::Response, ProxyError> {
    tracing::warn!(
        status = %status,
        body = %crate::errors::truncate(text, 1024),
        "upstream non-2xx on stream request"
    );
    Err(ProxyError::new(ErrorKind::Upstream, "upstream returned an error status")
        .with_field("status", status.to_string()))
}

/// Marqueur du 400 DeepSeek direct : `response_format` présent dans le corps
/// ET corps d'erreur « response_format type is unavailable » — les DEUX
/// conditions (jamais de strip sur un 400 quelconque).
const RESPONSE_FORMAT_UNAVAILABLE: &str = "response_format type is unavailable";

fn response_format_400(status: u16, body: &serde_json::Value, text: &str) -> bool {
    status == 400
        && body.get("response_format").is_some()
        && text.contains(RESPONSE_FORMAT_UNAVAILABLE)
}

/// Dégradation ciblée : retire `response_format` du corps (le seul champ
/// touché — le reste, déjà tokenisé, est transmis à l'identique).
fn strip_response_format(body: &serde_json::Value) -> serde_json::Value {
    let mut stripped = body.clone();
    if let Some(obj) = stripped.as_object_mut() {
        obj.remove("response_format");
    }
    stripped
}

#[cfg(test)]
mod url_tests {
    use super::*;
    use url::Url;

    fn cfg(base: &str, path: &str) -> UpstreamClient {
        let c = crate::config::UpstreamConfig {
            base_url: Url::parse(base).unwrap(),
            chat_completions_path: path.to_string(),
            completions_path: "/v1/completions".to_string(),
            models_path: "/v1/models".to_string(),
            connect_timeout: std::time::Duration::from_secs(1),
            request_timeout: std::time::Duration::from_secs(1),
            max_body_bytes: 1048576,
        };
        UpstreamClient::new(&c).unwrap()
    }

    #[test]
    fn base_without_v1_prefix() {
        let u = cfg("https://openrouter.ai", "/v1/chat/completions");
        let url = u.url("/v1/chat/completions").unwrap();
        assert_eq!(url.as_str(), "https://openrouter.ai/v1/chat/completions");
    }

    #[test]
    fn base_with_api_v1_no_double() {
        let u = cfg("https://openrouter.ai/api/v1", "/v1/chat/completions");
        let url = u.url("/v1/chat/completions").unwrap();
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn base_with_api_v1_relative_path() {
        let u = cfg("https://openrouter.ai/api/v1", "/chat/completions");
        let url = u.url("/chat/completions").unwrap();
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn strip_response_format_removes_only_that_key() {
        let body = serde_json::json!({
            "model": "deepseek-chat",
            "response_format": {"type": "json_object"},
            "messages": [{"role": "user", "content": "ok"}],
        });
        let stripped = strip_response_format(&body);
        assert!(stripped.get("response_format").is_none());
        assert_eq!(stripped["model"], "deepseek-chat");
        assert_eq!(stripped["messages"][0]["content"], "ok");
    }

    #[test]
    fn response_format_400_requires_marker_and_field() {
        let body = serde_json::json!({"response_format": {"type": "json_object"}});
        let marker = r#"{"error":{"message":"This response_format type is unavailable now"}}"#;
        assert!(response_format_400(400, &body, marker));
        assert!(!response_format_400(400, &serde_json::json!({"model": "x"}), marker));
        assert!(!response_format_400(400, &body, r#"{"error":{"message":"autre"}}"#));
        assert!(!response_format_400(500, &body, marker));
    }
}
