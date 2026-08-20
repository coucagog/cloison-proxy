//! Flux SSE OpenAI (`data: {json}\n\n` … `data: [DONE]`) avec buffer-and-scan.
//!
//! Une sentinelle CLOISON peut être émise par le LLM en morceaux arbitraires.
//! Le proxy ne doit émettre vers le client que du **texte confirmé** : tout ce
//! qui pourrait être le début d'une sentinelle reste en tampon (borné), la
//! résolution se fait au fil de l'eau (`restore` sur le registre vivant) et à
//! la clôture. Fail-loud : jeton non résoluble → marqueur neutre + compteur.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use tracing::warn;

use cloison_core::Sentinel;

use crate::engine::RequestEngine;
use crate::handlers::AppState;

/// Constantes du protocole SSE.
pub const SSE_DATA_PREFIX: &str = "data: ";
pub const SSE_EVENT_SEP: &str = "\n\n";
pub const SSE_DONE: &str = "data: [DONE]";

/// Taille max d'une sentinelle CLOISON (défaut 64 ; plafond dur 256).
pub const DEFAULT_MAX_TOKEN_LEN: usize = 64;
pub const MAX_TOKEN_LEN_HARD_CAP: usize = 256;

/// Marqueur neutre par défaut (fail-loud).
pub const NEUTRAL_MARKER: &str = "[REDACTED]";

/// Nombre maximal de buffers d'arguments d'outils in-flight par flux.
const MAX_TOOL_CALL_BUFFERS: usize = 32;
/// Taille maximale d'un événement SSE amont (octets) avant flush forcé.
const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Compteurs fail-loud d'une clôture de flux (ou d'une réponse non-stream).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FinishStats {
    /// Jetons restaurés.
    pub restored: usize,
    /// Jetons non résolus → marqueur neutre émis.
    pub unresolved: usize,
    /// Sentinelles rejetées (hors registre / MAC invalide / malformées).
    pub blocked: usize,
}

impl std::ops::Add for FinishStats {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            restored: self.restored + other.restored,
            unresolved: self.unresolved + other.unresolved,
            blocked: self.blocked + other.blocked,
        }
    }
}

impl std::ops::AddAssign for FinishStats {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

/// Tampon de lecture d'un flux amont, canal par canal
/// (`delta.content`, `delta.tool_calls[i].function.arguments`).
///
/// Règles d'émission :
/// - on garde toujours les `max_token_len - 1` derniers octets en tampon
///   (aucune sentinelle complète ne peut chevaucher la frontière) ;
/// - un `⟦` non fermé dans la région à émettre retient tout ce qui suit
///   (tant qu'une `⟧` peut encore arriver dans `max_token_len` octets) ;
/// - la région confirmée est restaurée (registre de la requête) avant émission ;
/// - le tampon est borné (~`2 × max_token_len` dans le pire cas).
pub struct BufferAndScan {
    /// Suffixe ambigu : ≤ ~`max_token_len` octets.
    buffer: String,
    /// Moteur de LA requête en cours (registre d'émission vivant).
    engine: Arc<Mutex<RequestEngine>>,
    request_id: String,
    max_token_len: usize,
    neutral_marker: String,
    /// Compteurs fail-loud accumulés (mid-stream + clôture).
    stats: FinishStats,
}

impl BufferAndScan {
    /// Construit un tampon pour un canal.
    pub fn new(
        engine: Arc<Mutex<RequestEngine>>,
        request_id: String,
        max_token_len: usize,
        neutral_marker: String,
    ) -> Self {
        Self {
            buffer: String::new(),
            engine,
            request_id,
            max_token_len: max_token_len.clamp(1, MAX_TOKEN_LEN_HARD_CAP),
            neutral_marker,
            stats: FinishStats::default(),
        }
    }

    /// Absorbe un fragment de texte brut (delta) et émet le texte confirmé
    /// restauré via `emit`.
    pub fn push(&mut self, fragment: &str, emit: &mut impl FnMut(&str)) {
        self.buffer.push_str(fragment);

        // Budget d'octets : garder au moins `max_token_len - 1` octets en
        // tampon (une sentinelle complète y tient entièrement).
        let max_safe = self.buffer.len().saturating_sub(self.max_token_len - 1);
        let mut emit_len = max_safe;
        // Ne jamais couper un caractère UTF-8 multi-octets.
        while emit_len > 0 && !self.buffer.is_char_boundary(emit_len) {
            emit_len -= 1;
        }

        // Retenir tout `⟦` non fermé dans la région (et ce qui suit) tant
        // qu'une `⟧` peut encore arriver dans `max_token_len` octets.
        // Itératif : les `⟦` imbriqués sont traités du plus récent au plus ancien.
        while let Some(open) = self.buffer[..emit_len].rfind(Sentinel::L_OPEN) {
            let after_open = open + Sentinel::L_OPEN.len_utf8();
            if self.buffer[after_open..emit_len].contains(Sentinel::L_CLOSE) {
                // Le dernier `⟦` de la région est fermé → tous les précédents
                // le sont aussi (leur `⟧` suit nécessairement).
                break;
            }
            let close_soon = self
                .buffer[after_open..]
                .find(Sentinel::L_CLOSE)
                .map(|rel| rel + after_open - open < self.max_token_len)
                .unwrap_or(false);
            if !close_soon {
                // `⟦` isolé (aucune `⟧` dans `max_token_len` octets) : ce n'est
                // pas un préfixe de sentinelle valide → sûr à émettre.
                break;
            }
            emit_len = open;
        }

        if emit_len > 0 {
            let region = self.buffer[..emit_len].to_string();
            self.buffer.drain(..emit_len);
            let restored = self.restore_region(&region);
            emit(&restored);
        }
    }

    /// À la clôture : résout le tampon résiduel (fail-loud) et renvoie les
    /// compteurs totaux du canal (mid-stream + clôture).
    pub fn finish(&mut self, emit: &mut impl FnMut(&str)) -> FinishStats {
        let residual = std::mem::take(&mut self.buffer);
        if !residual.is_empty() {
            let engine = match self.engine.lock() {
                Ok(g) => g,
                Err(_) => {
                    self.stats.unresolved += 1;
                    emit(&self.neutral_marker);
                    return self.stats;
                }
            };
            match engine.restore(&residual) {
                Ok(r) => {
                    // cloison-core signale toute ouverture non fermee (tronquee)
                    // via blocked/incomplete : le check `unclosed` local serait
                    // un double comptage. On se fie au restore.
                    if r.counters.blocked == 0 && r.counters.incomplete == 0 {
                        // Cas nominal : texte clair et/ou sentinelle complète résolue.
                        emit(&r.text_out);
                        self.stats.restored += r.counters.restored;
                    } else {
                        // Fail-loud : sentinelle tronquée / invalide / hors
                        // registre → marqueur neutre, AUCUNE valeur claire ne fuit.
                        let n = r.counters.blocked + r.counters.incomplete;
                        emit(&self.neutral_marker);
                        self.stats.unresolved += n.max(1);
                        self.stats.blocked += r.counters.blocked;
                        warn!(
                            request_id = %self.request_id,
                            restored = r.counters.restored,
                            unresolved = n,
                            blocked = r.counters.blocked,
                            "stream closure: unresolved token redacted (fail-loud)"
                        );
                    }
                }
                Err(e) => {
                    self.stats.unresolved += 1;
                    emit(&self.neutral_marker);
                    warn!(request_id = %self.request_id, error = %e, "stream closure restore failure: redacting residual");
                }
            }
        }
        self.stats
    }

    /// Restaure une région confirmée ; échec → marqueur neutre (fail-loud).
    fn restore_region(&mut self, region: &str) -> String {
        let engine = match self.engine.lock() {
            Ok(g) => g,
            Err(_) => {
                self.stats.unresolved += 1;
                return self.neutral_marker.clone();
            }
        };
        match engine.restore(region) {
            Ok(r) => {
                if r.counters.blocked == 0 && r.counters.incomplete == 0 {
                    self.stats.restored += r.counters.restored;
                    r.text_out
                } else {
                    let n = r.counters.blocked + r.counters.incomplete;
                    self.stats.unresolved += n.max(1);
                    self.stats.blocked += r.counters.blocked;
                    warn!(
                        request_id = %self.request_id,
                        unresolved = n,
                        blocked = r.counters.blocked,
                        "mid-stream restore failure: redacting region (fail-loud)"
                    );
                    self.neutral_marker.clone()
                }
            }
            Err(e) => {
                self.stats.unresolved += 1;
                warn!(request_id = %self.request_id, error = %e, "mid-stream restore failure: redacting region");
                self.neutral_marker.clone()
            }
        }
    }
}

/// Trouve la fin du premier événement SSE (`\n\n` ou `\r\n\r\n`).
/// Renvoie `(position, longueur_du_séparateur)`.
fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
        return Some((pos, 2));
    }
    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some((pos, 4));
    }
    None
}

/// Extrait la charge utile `data:` d'un événement SSE brut
/// (plusieurs lignes `data:` concaténées par `\n`).
fn event_data_payload(event: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(event);
    let mut parts = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("data:") {
            parts.push(rest.trim_start().to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Réponse SSE finale du proxy (flux interne muni du keep-alive).
pub type SseResponse = Sse<
    axum::response::sse::KeepAliveStream<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>,
>;

/// Métadonnées de chunk reconstruites : (id, object, created, model).
type Meta = (String, String, u64, String);

fn default_meta() -> Meta {
    ("".to_string(), "chat.completion.chunk".to_string(), 0, "".to_string())
}

/// Assemble la réponse SSE : découpe le corps amont en événements `data:`,
/// route chaque delta vers le bon `BufferAndScan`, reconstruit les événements
/// restaurés, applique keep-alive et émet `data: [DONE]` à la fin.
pub fn sse_response(
    upstream: reqwest::Response,
    state: Arc<AppState>,
    request_id: String,
    engine: Arc<Mutex<RequestEngine>>,
) -> SseResponse {
    let max_token_len = state.stream_cfg.max_token_len;
    let neutral_marker = state.stream_cfg.neutral_marker.clone();
    let keep_alive = state.stream_cfg.keep_alive;

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = Box::pin(async_stream::stream! {
        let mut content_buf = BufferAndScan::new(engine.clone(), request_id.clone(), max_token_len, neutral_marker.clone());
        let mut tool_bufs: Vec<BufferAndScan> = Vec::new();
        let mut meta: Meta = default_meta();

        // Lecture + découpage en événements SSE.
        let mut frame: Vec<u8> = Vec::new();
        let mut bytes = upstream.bytes_stream();
        let mut done = false;

        'outer: loop {
            let chunk = match bytes.next().await {
                Some(Ok(c)) => c,
                Some(Err(e)) => {
                    warn!(request_id = %request_id, error = %e, "upstream stream read error");
                    break;
                }
                None => break,
            };
            frame.extend_from_slice(&chunk);

            if frame.len() > MAX_FRAME_BYTES {
                // Événement amont démesuré : traite ce qui est déjà là puis purge.
                warn!(request_id = %request_id, bytes = frame.len(), "upstream SSE frame exceeded limit; force-flushing");
                let overflow: Vec<u8> = std::mem::take(&mut frame);
                if let Some(payload) = event_data_payload(&overflow) {
                    if payload != "[DONE]" {
                        let events = process_payload(&payload, &mut meta, &mut content_buf, &mut tool_bufs, &engine, &request_id, max_token_len, &neutral_marker);
                        for ev in events {
                            yield Ok::<Event, Infallible>(ev);
                        }
                    } else {
                        done = true;
                        break 'outer;
                    }
                }
                continue;
            }

            while let Some((end, sep_len)) = find_event_end(&frame) {
                let event_bytes: Vec<u8> = frame.drain(..end + sep_len).collect();
                if let Some(payload) = event_data_payload(&event_bytes) {
                    if payload == "[DONE]" {
                        done = true;
                        break 'outer;
                    }
                    let events = process_payload(&payload, &mut meta, &mut content_buf, &mut tool_bufs, &engine, &request_id, max_token_len, &neutral_marker);
                    for ev in events {
                        yield Ok::<Event, Infallible>(ev);
                    }
                }
            }
        }

        // Événement résiduel sans séparateur final (fin de flux propre).
        if !done {
            if let Some(payload) = event_data_payload(&frame) {
                if payload != "[DONE]" {
                    let events = process_payload(&payload, &mut meta, &mut content_buf, &mut tool_bufs, &engine, &request_id, max_token_len, &neutral_marker);
                    for ev in events {
                        yield Ok::<Event, Infallible>(ev);
                    }
                }
            }
        }

        // Clôture : résout les tampons résiduels, émet les fragments restants.
        let mut pending: Vec<Event> = Vec::new();
        let mut total = content_buf.finish(&mut |frag| {
            pending.push(content_event(&meta, frag));
        });
        for (idx, tb) in tool_bufs.iter_mut().enumerate() {
            total += tb.finish(&mut |frag| {
                pending.push(tool_args_event(&meta, idx, frag));
            });
        }
        if total.unresolved > 0 {
            state
                .metrics
                .unresolved_tokens
                .fetch_add(total.unresolved as u64, Ordering::Relaxed);
            warn!(
                request_id = %request_id,
                restored = total.restored,
                unresolved = total.unresolved,
                blocked = total.blocked,
                "stream closure: fail-loud redaction applied"
            );
        }
        for ev in pending {
            yield Ok::<Event, Infallible>(ev);
        }
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(keep_alive))
}

/// Traite un événement `data:` : route `delta.content` / `delta.tool_calls[].function.arguments`
/// vers les tampons, reconstruit les événements restaurés. Un événement sans
/// contenu ni arguments (role, finish_reason, delta vide, champs spécifiques)
/// est passé tel quel.
#[allow(clippy::too_many_arguments)]
fn process_payload(
    payload: &str,
    meta: &mut Meta,
    content_buf: &mut BufferAndScan,
    tool_bufs: &mut Vec<BufferAndScan>,
    engine: &Arc<Mutex<RequestEngine>>,
    request_id: &str,
    max_token_len: usize,
    neutral_marker: &str,
) -> Vec<Event> {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return Vec::new();
    };

    // Capture des métadonnées (id, object, created, model) à chaque événement.
    if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
        meta.0 = id.to_string();
    }
    if let Some(object) = value.get("object").and_then(|v| v.as_str()) {
        meta.1 = object.to_string();
    }
    if let Some(created) = value.get("created").and_then(|v| v.as_u64()) {
        meta.2 = created;
    }
    if let Some(model) = value.get("model").and_then(|v| v.as_str()) {
        meta.3 = model.to_string();
    }

    let Some(choices) = value.get("choices").and_then(|v| v.as_array()) else {
        return vec![Event::default().data(payload)];
    };
    let Some(choice) = choices.first() else {
        return vec![Event::default().data(payload)];
    };
    let Some(delta) = choice.get("delta").and_then(|v| v.as_object()) else {
        return vec![Event::default().data(payload)];
    };

    let mut pending: Vec<Event> = Vec::new();
    let mut routed = false;

    // Canal content.
    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
        routed = true;
        if !content.is_empty() {
            let mut frags: Vec<String> = Vec::new();
            content_buf.push(content, &mut |f| frags.push(f.to_string()));
            for f in frags {
                pending.push(content_event(meta, &f));
            }
        }
    }

    // Canaux tool_calls (un buffer par index).
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for (pos, tc) in tool_calls.iter().enumerate() {
            let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(pos as u64) as usize;
            if index >= MAX_TOOL_CALL_BUFFERS {
                warn!(request_id, index, "tool_call index exceeds cap; skipping");
                continue;
            }
            while tool_bufs.len() <= index {
                tool_bufs.push(BufferAndScan::new(
                    engine.clone(),
                    request_id.to_string(),
                    max_token_len,
                    neutral_marker.to_string(),
                ));
            }
            if let Some(args) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|v| v.as_str()) {
                routed = true;
                if !args.is_empty() {
                    let mut frags: Vec<String> = Vec::new();
                    tool_bufs[index].push(args, &mut |f| frags.push(f.to_string()));
                    for f in frags {
                        pending.push(tool_args_event(meta, index, &f));
                    }
                } else {
                    // Premier chunk d'un tool_call : il porte `id` et `name`
                    // avec `arguments:""`. Il ne doit PAS etre perdu — on
                    // l'emette tel quel (aucun argument a restaurer encore).
                    pending.push(Event::default().data(payload));
                }
            }
        }
    }

    if pending.is_empty() && !routed {
        // Rien de routé : role, finish_reason, delta vide, champs spécifiques → tel quel.
        vec![Event::default().data(payload)]
    } else {
        pending
    }
}

/// Événement `delta.content` restauré (shape §6.4 du design).
fn content_event(meta: &Meta, content: &str) -> Event {
    let (id, object, created, model) = meta;
    let payload = serde_json::json!({
        "id": id,
        "object": object,
        "created": created,
        "model": model,
        "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}],
    });
    Event::default().data(payload.to_string())
}

/// Événement `delta.tool_calls[i].function.arguments` restauré.
fn tool_args_event(meta: &Meta, index: usize, args: &str) -> Event {
    let (id, object, created, model) = meta;
    let payload = serde_json::json!({
        "id": id,
        "object": object,
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{"index": index, "function": {"arguments": args}}]},
            "finish_reason": null,
        }],
    });
    Event::default().data(payload.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_end_detection() {
        assert_eq!(find_event_end(b"data: x\n\n"), Some((7, 2)));
        assert_eq!(find_event_end(b"data: x\r\n\r\n"), Some((7, 4)));
        assert_eq!(find_event_end(b"data: x"), None);
    }

    #[test]
    fn data_payload_extraction() {
        assert_eq!(event_data_payload(b"data: {}\n\n").as_deref(), Some("{}"));
        assert_eq!(event_data_payload(b"data: [DONE]\n\n").as_deref(), Some("[DONE]"));
        assert_eq!(event_data_payload(b"data: a\ndata: b\n\n").as_deref(), Some("a\nb"));
        assert_eq!(event_data_payload(b": keep-alive\n\n"), None);
    }

}
