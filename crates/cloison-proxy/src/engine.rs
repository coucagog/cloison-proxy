//! Pont vers `cloison-core` : tokenisation aller / restauration retour,
//! périmètre = **la requête en cours** (un moteur par requête, registre
//! d'émission vivant du tokenize au restore — invariant I2).
//!
//! Chaque requête possède son propre `Engine` (`Engine::new(keys)`) : le
//! registre d'émission ne contient que les jetons de CETTE requête, il n'y a
//! donc ni purge ni course entre requêtes concurrentes.

use serde_json::Value;

use cloison_core::{CloisonError, CloisonResult, Engine, Policy, RestoreResult, SessionKeys, TokenizeResult};

use crate::errors::{ErrorKind, ProxyError};
use crate::handlers::Metrics;
use crate::openai::{
    ChatCompletionRequest, ChatCompletionResponse, CompletionRequest, Content, Prompt,
};

/// Compteurs agrégés d'une restauration non-stream (fail-loud).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreAggregate {
    /// Jetons restaurés.
    pub restored: usize,
    /// Jetons non résolus → marqueur neutre.
    pub unresolved: usize,
}

/// Moteur d'une requête : `tokenize` (aller) / `restore` (retour) sur le même
/// registre d'émission.
pub struct RequestEngine {
    engine: Engine,
    request_id: String,
}

impl RequestEngine {
    /// Crée un moteur vierge pour une requête (registre vide).
    pub fn new(keys: &SessionKeys, request_id: &str) -> Result<Self, ProxyError> {
        let engine = Engine::new(keys.clone())
            .map_err(|e| ProxyError::new(ErrorKind::Internal, "failed to initialize engine").with_field("detail", e.to_string()))?;
        Ok(Self {
            engine,
            request_id: request_id.to_string(),
        })
    }

    /// Tokenise un texte (aller) : détection PII + remplacement par sentinelles.
    pub fn tokenize(&mut self, text: &str, policy: &Policy) -> CloisonResult<TokenizeResult> {
        self.engine.tokenize(text, policy, &self.request_id)
    }

    /// Restaure les jetons émis par cette requête (retour).
    pub fn restore(&self, text: &str) -> CloisonResult<RestoreResult> {
        self.engine.restore(text, &self.request_id)
    }
}

/// Tokenise `messages[].content` (Text + Parts.text) et
/// `tool_calls[].function.arguments` — aller.
pub fn tokenize_chat_request(
    req: &mut ChatCompletionRequest,
    engine: &mut RequestEngine,
    policy: &Policy,
) -> Result<(), ProxyError> {
    for msg in &mut req.messages {
        if let Some(content) = &mut msg.content {
            match content {
                Content::Text(s) => {
                    let r = engine.tokenize(s, policy).map_err(proxy_internal)?;
                    *s = r.text_out;
                }
                Content::Parts(parts) => {
                    for part in parts {
                        if part.type_ == "text" {
                            if let Some(text) = &mut part.text {
                                let r = engine.tokenize(text, policy).map_err(proxy_internal)?;
                                *text = r.text_out;
                            }
                        }
                    }
                }
            }
        }
        if let Some(calls) = &mut msg.tool_calls {
            for call in calls {
                let r = engine.tokenize(&call.function.arguments, policy).map_err(proxy_internal)?;
                call.function.arguments = r.text_out;
            }
        }
    }
    Ok(())
}

/// Tokenise `prompt` (String ou Vec<String>) — aller (legacy).
pub fn tokenize_completion_request(
    req: &mut CompletionRequest,
    engine: &mut RequestEngine,
    policy: &Policy,
) -> Result<(), ProxyError> {
    match &mut req.prompt {
        Prompt::Single(s) => {
            let r = engine.tokenize(s, policy).map_err(proxy_internal)?;
            *s = r.text_out;
        }
        Prompt::Batch(v) => {
            for s in v {
                let r = engine.tokenize(s, policy).map_err(proxy_internal)?;
                *s = r.text_out;
            }
        }
    }
    Ok(())
}

/// Restaure `choices[].message.content` + `choices[].message.tool_calls[].function.arguments`
/// dans une réponse typée. Fail-loud : champ → marqueur neutre si un jeton est
/// non résolu (bloqué ou incomplet), compteur incrémenté.
pub fn restore_chat_response(
    resp: &mut ChatCompletionResponse,
    engine: &RequestEngine,
    neutral_marker: &str,
) -> Result<RestoreAggregate, ProxyError> {
    let mut agg = RestoreAggregate::default();
    for choice in &mut resp.choices {
        if let Some(content) = &mut choice.message.content {
            match content {
                Content::Text(s) => apply_restore(s, engine, neutral_marker, &mut agg)?,
                Content::Parts(parts) => {
                    for part in parts {
                        if part.type_ == "text" {
                            if let Some(text) = &mut part.text {
                                apply_restore(text, engine, neutral_marker, &mut agg)?;
                            }
                        }
                    }
                }
            }
        }
        if let Some(calls) = &mut choice.message.tool_calls {
            for call in calls {
                if let Some(args) = &mut call.function.arguments {
                    apply_restore(args, engine, neutral_marker, &mut agg)?;
                }
            }
        }
    }
    Ok(agg)
}

/// Restaure une réponse amont (Value) : parse la shape `chat.completion`,
/// restaure, re-sérialise. Toute shape inconnue est transmise **telle quelle**
/// (pass-through conservateur, I6) — jamais de 500 sur une réponse exotique.
pub fn restore_chat_response_value(
    resp: Value,
    engine: &RequestEngine,
    neutral_marker: &str,
    metrics: &Metrics,
    request_id: &str,
) -> Result<Value, ProxyError> {
    match serde_json::from_value::<ChatCompletionResponse>(resp.clone()) {
        Ok(mut typed) => {
            let agg = restore_chat_response(&mut typed, engine, neutral_marker)?;
            if agg.unresolved > 0 {
                metrics
                    .unresolved_tokens
                    .fetch_add(agg.unresolved as u64, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(
                    request_id,
                    restored = agg.restored,
                    unresolved = agg.unresolved,
                    "non-stream restore: fail-loud redaction applied"
                );
            }
            serde_json::to_value(typed)
                .map_err(|e| ProxyError::new(ErrorKind::Internal, "failed to serialize restored response").with_field("detail", e.to_string()))
        }
        Err(e) => {
            tracing::warn!(request_id, error = %e, "upstream response shape not recognized; passing through untouched");
            Ok(resp)
        }
    }
}

/// Restaure `choices[].text` (legacy). Fail-loud identique.
pub fn restore_completion_response(
    resp: &mut Value,
    engine: &RequestEngine,
    neutral_marker: &str,
) -> Result<RestoreAggregate, ProxyError> {
    let mut agg = RestoreAggregate::default();
    if let Some(choices) = resp.get_mut("choices").and_then(|c| c.as_array_mut()) {
        for choice in choices {
            if let Some(text) = choice.get("text").and_then(|t| t.as_str()).map(str::to_string) {
                let r = engine.restore(&text).map_err(proxy_internal)?;
                if r.counters.blocked + r.counters.incomplete > 0 {
                    choice["text"] = Value::String(neutral_marker.to_string());
                } else {
                    choice["text"] = Value::String(r.text_out);
                }
                agg.restored += r.counters.restored;
                agg.unresolved += r.counters.blocked + r.counters.incomplete;
            }
        }
    }
    Ok(agg)
}

/// Applique une restauration fail-loud sur un champ unique.
fn apply_restore(
    field: &mut String,
    engine: &RequestEngine,
    neutral_marker: &str,
    agg: &mut RestoreAggregate,
) -> Result<(), ProxyError> {
    let r = engine.restore(field).map_err(proxy_internal)?;
    if r.counters.blocked + r.counters.incomplete > 0 {
        *field = neutral_marker.to_string();
    } else {
        *field = r.text_out;
    }
    agg.restored += r.counters.restored;
    agg.unresolved += r.counters.blocked + r.counters.incomplete;
    Ok(())
}

/// Convertit une erreur `cloison-core` en 500 interne (jamais silencieux).
pub(crate) fn proxy_internal(e: CloisonError) -> ProxyError {
    ProxyError::new(ErrorKind::Internal, "internal tokenization error").with_field("detail", e.to_string())
}
