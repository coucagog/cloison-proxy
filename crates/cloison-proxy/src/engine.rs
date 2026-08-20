//! Pont vers `cloison-core` : tokenisation aller / restauration retour,
//! périmètre = **la requête en cours** (un moteur par requête, registre
//! d'émission vivant du tokenize au restore — invariant I2).
//!
//! Chaque requête possède son propre `Engine` (`Engine::new(keys)`) : le
//! registre d'émission ne contient que les jetons de CETTE requête, il n'y a
//! donc ni purge ni course entre requêtes concurrentes.
//!
//! STACK-4 : `AuditEngine` — moteur d'audit **observe-only**. Il réutilise le
//! `Detector` de cloison-core pour **compter** les PII sans rien masquer :
//! le texte passe au client tel quel, seuls les compteurs sont accumulés
//! dans des reçus signés (jamais de texte).

use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use cloison_audit::ed25519_dalek::{SigningKey, VerifyingKey};
use cloison_audit::receipt::{self, Counters, Receipt, ReceiptMessage};
use cloison_audit::report::ConformanceReport;
use cloison_core::{
    CloisonError, CloisonResult, Detector, DetectorKind, Engine, Policy, RestoreResult,
    SessionKeys, Sentinel, TokenizeResult,
};

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

// ---------------------------------------------------------------------------
// STACK-4 — moteur d'audit observe-only
// ---------------------------------------------------------------------------

/// Types à faible cardinalité généralisés par `cloison-core` (jamais
/// tokenisés) : leurs occurrences sont des drapeaux quasi-identifiants.
fn is_quasi_identifier(kind: &DetectorKind) -> bool {
    matches!(kind, DetectorKind::Ip | DetectorKind::Date | DetectorKind::CreditCard)
}

/// Moteur d'audit observe-only : détecte et **compte** sans jamais masquer.
///
/// - Réutilise le `Detector` de cloison-core (même configuration que le mode
///   masquage) ;
/// - `count_text` incrémente `masked_by_type` (masqué *potentiel*) et
///   `quasi_id_flags` (types généralisés) **sans modifier le texte** ;
/// - `count_response` compte les sentinelles d'une réponse (aucune n'est
///   légitime en mode audit) ;
/// - chaque requête produit un `Receipt` signé Ed25519 (clé de l'agent au
///   bord), accumulé dans `ledger` pour le rapport de conformité.
///
/// Le reçu ne contient **jamais de texte** : uniquement des compteurs.
pub struct AuditEngine {
    /// Détecteur partagé avec le mode masquage (STACK-2).
    detector: Detector,
    /// Clé de signature Ed25519 de l'agent au bord.
    signing_key: SigningKey,
    /// hex(public_key[0..8]) — identifiant de clé (rotation).
    key_id: String,
    /// hex(SHA-256(json canonique de la Policy)) — règle appliquée au comptage.
    policy_hash: String,
    /// Version du moteur (`CARGO_PKG_VERSION` du proxy).
    engine_version: String,
    /// Seuil k-anonyme du rapport (défaut 5).
    k: usize,
    /// Journal des reçus accumulés (pour le rapport) — compteurs uniquement.
    ledger: Mutex<Vec<Receipt>>,
}

impl AuditEngine {
    /// Construit le moteur : détecteur par défaut, clé chargée ou générée,
    /// hash de politique, seuil k validé (≥ 2).
    pub fn new(policy: &Policy, keys_path: Option<&Path>, k: usize) -> Result<Self, ProxyError> {
        if k < 2 {
            return Err(ProxyError::new(
                ErrorKind::Internal,
                "audit k-anonymity threshold must be >= 2 (CLOISON_AUDIT_K)",
            ));
        }
        let detector = Detector::new().map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to initialize audit detector")
                .with_field("detail", e.to_string())
        })?;
        let signing_key = load_or_create_signing_key(keys_path)?;
        let key_id = hex_short(&signing_key.verifying_key().to_bytes());
        let policy_hash = receipt::policy_hash(policy).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to hash audit policy")
                .with_field("detail", e.to_string())
        })?;
        Ok(Self {
            detector,
            signing_key,
            key_id,
            policy_hash,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            k,
            ledger: Mutex::new(Vec::new()),
        })
    }

    /// Clé publique de l'agent (vérification hors-ligne).
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Identifiant de clé (hex des 8 premiers octets de la clé publique).
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Hash de politique appliquée au comptage.
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }

    /// Version du moteur.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// Compte les PII d'un texte (masqué *potentiel*), **sans le modifier**.
    ///
    /// Les valeurs des spans ne sortent jamais d'ici : seuls les compteurs
    /// sont incrémentés.
    pub fn count_text(&self, text: &str, policy: &Policy, counters: &mut Counters) {
        for span in self.detector.detect_with_policy(text, &policy.detection) {
            if is_quasi_identifier(&span.entity_type) {
                counters.quasi_id_flags += 1;
            } else {
                *counters
                    .masked_by_type
                    .entry(span.entity_type.to_string())
                    .or_insert(0) += 1;
            }
        }
    }

    /// Compte les sentinelles d'un texte de réponse.
    ///
    /// En mode audit aucune sentinelle n'est légitime (registre d'émission
    /// vide) : une sentinelle bien formée mais non résolvable →
    /// `incomplete_restorations` ; une forme invalide → `blocked_outputs`
    /// (en mode masquage, ce champ aurait été passé au marqueur neutre).
    pub fn count_response(&self, text: &str, counters: &mut Counters) {
        for shape in sentinel_shapes(text) {
            match Sentinel::parse(&shape) {
                Some(_) => counters.incomplete_restorations += 1,
                None => counters.blocked_outputs += 1,
            }
        }
    }

    /// Construit un reçu **non signé** pour cette requête.
    pub fn build_receipt(
        &self,
        tenant_id: String,
        session_ref_hashed: String,
        ts_unix: u64,
        counters: Counters,
    ) -> Receipt {
        Receipt::build(ReceiptMessage {
            tenant_id,
            session_ref_hashed,
            ts_unix,
            engine_version: self.engine_version.clone(),
            policy_hash: self.policy_hash.clone(),
            counters,
        })
    }

    /// Signe un reçu avec la clé de l'agent au bord.
    pub fn sign(&self, receipt: &Receipt) -> Receipt {
        receipt.sign(&self.signing_key)
    }

    /// Accumule un reçu signé dans le journal (pour le rapport).
    pub fn record(&self, receipt: Receipt) {
        self.ledger.lock().expect("audit ledger mutex poisoned").push(receipt);
    }

    /// Nombre de reçus accumulés.
    pub fn receipts_len(&self) -> usize {
        self.ledger.lock().expect("audit ledger mutex poisoned").len()
    }

    /// Copie des reçus accumulés (tests, vérification hors-ligne).
    pub fn receipts(&self) -> Vec<Receipt> {
        self.ledger.lock().expect("audit ledger mutex poisoned").clone()
    }

    /// Rapport de conformité k-anonyme sur le journal accumulé.
    ///
    /// Bornes de période : `[min(ts), max(ts)+1)` ; si aucun reçu, `[0, now]`.
    pub fn report(&self) -> Result<ConformanceReport, ProxyError> {
        let receipts = self.receipts();
        let period_start = receipts.iter().map(|r| r.ts_unix).min().unwrap_or(0);
        let period_end = receipts
            .iter()
            .map(|r| r.ts_unix)
            .max()
            .map(|m| m + 1)
            .unwrap_or_else(receipt::now_unix);
        let mut report =
            ConformanceReport::from_receipts(&receipts, period_start, period_end, self.k).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to build conformance report")
                    .with_field("detail", e.to_string())
            })?;
        // P0-3 : le rapport servi est signé par la clé de l'agent au bord.
        // Message = JSON canonique {period_start, period_end, total_requests,
        // redacted} — jamais les compteurs bruts (aggregated).
        report.sign_report(&self.signing_key);
        Ok(report)
    }
}

/// Charge la clé de signature : fichier existant (32 octets bruts ou 64 hex),
/// sinon génération + écriture 0600 ; sans chemin, clé éphémère (warning).
fn load_or_create_signing_key(path: Option<&Path>) -> Result<SigningKey, ProxyError> {
    match path {
        Some(p) if p.exists() => {
            let raw = std::fs::read(p).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to read audit key file")
                    .with_field("path", p.display().to_string())
                    .with_field("detail", e.to_string())
            })?;
            let seed: [u8; 32] = if raw.len() == 32 {
                let mut s = [0u8; 32];
                s.copy_from_slice(&raw);
                s
            } else if raw.len() == 64 {
                decode_seed_hex(&String::from_utf8_lossy(&raw)).ok_or_else(|| {
                    ProxyError::new(ErrorKind::Internal, "audit key file must be 32 raw bytes or 64 hex chars")
                        .with_field("path", p.display().to_string())
                })?
            } else {
                return Err(ProxyError::new(
                    ErrorKind::Internal,
                    "audit key file must be 32 raw bytes or 64 hex chars",
                )
                .with_field("path", p.display().to_string()));
            };
            Ok(SigningKey::from_bytes(&seed))
        }
        Some(p) => {
            let key = SigningKey::generate(&mut rand::rngs::OsRng);
            write_seed_file(p, &key.to_bytes())?;
            tracing::info!(path = %p.display(), "audit agent key generated and written (0600)");
            Ok(key)
        }
        None => {
            tracing::warn!("audit mode enabled without CLOISON_AUDIT_KEYS: using an ephemeral signing key");
            Ok(SigningKey::generate(&mut rand::rngs::OsRng))
        }
    }
}

/// Écrit une graine 32 octets avec permissions 0600 (jamais de logs dessus).
fn write_seed_file(path: &Path, seed: &[u8; 32]) -> Result<(), ProxyError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to create audit key file")
                .with_field("path", path.display().to_string())
                .with_field("detail", e.to_string())
        })?;
    file.write_all(seed).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to write audit key file")
            .with_field("path", path.display().to_string())
            .with_field("detail", e.to_string())
    })?;
    file.flush().map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to flush audit key file")
            .with_field("path", path.display().to_string())
            .with_field("detail", e.to_string())
    })
}

/// Décodage strict de 64 caractères hex → 32 octets.
fn decode_seed_hex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = [0u8; 32];
    for (i, chunk) in bytes.chunks(2).enumerate() {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// hex des 8 premiers octets.
fn hex_short(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Formes `⟦…⟧` présentes dans un texte (paires délimiteurs) — pour comptage.
fn sentinel_shapes(text: &str) -> Vec<String> {
    let mut shapes = Vec::new();
    let mut search = 0;
    while search < text.len() {
        let Some(rel_open) = text[search..].find(Sentinel::L_OPEN) else {
            break;
        };
        let open = search + rel_open;
        let after_open = open + Sentinel::L_OPEN.len_utf8();
        let Some(rel_close) = text[after_open..].find(Sentinel::L_CLOSE) else {
            break;
        };
        let close = after_open + rel_close + Sentinel::L_CLOSE.len_utf8();
        shapes.push(text[open..close].to_string());
        search = close;
    }
    shapes
}
