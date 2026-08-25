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

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use cloison_audit::ed25519_dalek::{SigningKey, VerifyingKey};
use cloison_audit::receipt::{self, Counters, Receipt, ReceiptMessage};
use cloison_audit::report::ConformanceReport;
use cloison_core::{
    CloisonError, CloisonResult, Detector, DetectorKind, Engine, Policy, RestoreResult, Sentinel,
    SessionContext, SessionKeys, SessionOptions, Span, TokenizeResult,
};

use crate::detect::DetectClient;
use crate::errors::{ErrorKind, ProxyError};
use crate::handlers::Metrics;
use crate::light_ner::LightNer;
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

/// Drapeaux de la session (N0 v1.1) remontés par la tokenisation aller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionFlags {
    /// La jauge quasi-id a signalé une densité élevée (compteur, jamais de
    /// texte — charte §6.1 couche 6 : signal, pas résolution).
    pub quasi_id_flagged: bool,
}

/// Moteur d'une requête : `tokenize` (aller) / `restore` (retour) sur le même
/// registre d'émission.
pub struct RequestEngine {
    engine: Engine,
    request_id: String,
}

impl RequestEngine {
    /// Crée un moteur vierge pour une requête (registre vide).
    ///
    /// `vault` (N0) : coffre persistant partagé — la restauration reste bornée
    /// au registre de la requête (invariant I3) ; le coffre fournit la valeur
    /// en fallback (source de vérité persistante, charte §9.1).
    pub fn new(
        keys: &SessionKeys,
        request_id: &str,
        vault: Option<std::sync::Arc<cloison_core::Vault>>,
    ) -> Result<Self, ProxyError> {
        let engine = match vault {
            Some(v) => Engine::with_vault(keys.clone(), (*v).clone()).map_err(|e| {
                ProxyError::new(
                    ErrorKind::Internal,
                    "failed to initialize engine with vault",
                )
                .with_field("detail", e.to_string())
            })?,
            None => Engine::new(keys.clone()).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to initialize engine")
                    .with_field("detail", e.to_string())
            })?,
        };
        Ok(Self {
            engine,
            request_id: request_id.to_string(),
        })
    }

    /// Tokenise un texte (aller) : détection PII + remplacement par sentinelles.
    pub fn tokenize(&mut self, text: &str, policy: &Policy) -> CloisonResult<TokenizeResult> {
        self.engine.tokenize(text, policy, &self.request_id)
    }

    /// Tokenise avec des spans NER externes (sidecar detect, B.1) : le cœur
    /// les valide et les fusionne avec sa détection embarquée.
    pub fn tokenize_with_extra(
        &mut self,
        text: &str,
        policy: &Policy,
        extra: &[Span],
    ) -> CloisonResult<TokenizeResult> {
        self.engine
            .tokenize_with_extra(text, policy, &self.request_id, extra)
    }

    /// Tokenise avec la **session** (N0 v1.1) : alias intra-session R1–R7 +
    /// jauge quasi-id in-core, en plus de la détection embarquée et des spans
    /// NER externes. La session (mentions canoniques) persiste entre les
    /// requêtes du daemon — jamais de texte hors du moteur.
    pub fn tokenize_session(
        &mut self,
        text: &str,
        policy: &Policy,
        session: &mut SessionContext,
        extra: &[Span],
        options: &SessionOptions,
    ) -> CloisonResult<TokenizeResult> {
        self.engine
            .tokenize_session(text, policy, &self.request_id, extra, session, options)
    }

    /// Restaure les jetons émis par cette requête (retour).
    pub fn restore(&self, text: &str) -> CloisonResult<RestoreResult> {
        self.engine.restore(text, &self.request_id)
    }
}

/// Tokenise `messages[].content` (Text + Parts.text) et
/// `tool_calls[].function.arguments` — aller.
///
/// B.1 : quand un client detect est configuré, chaque champ est d'abord
/// soumis au sidecar NER (appel réseau) puis tokenisé avec les spans
/// fusionnés. Une panne du sidecar dégrade en détection embarquée seule
/// (warn, jamais d'erreur) — le proxy ne tombe jamais à cause de detect.
///
/// N0 v1.1 : quand `session`/`options` sont fournis (mode N0), la
/// tokenisation passe par `Engine::tokenize_session` — alias intra-session
/// et jauge quasi-id in-core ; les drapeaux de session sont collectés sur
/// tous les champs (une seule occurrence flaggée suffit).
pub async fn tokenize_chat_request(
    req: &mut ChatCompletionRequest,
    engine: &mut RequestEngine,
    policy: &Policy,
    detect: Option<&DetectClient>,
    light_ner: Option<&LightNer>,
    mut session: Option<&mut SessionContext>,
    options: Option<&SessionOptions>,
) -> Result<SessionFlags, ProxyError> {
    let mut flags = SessionFlags::default();
    for msg in &mut req.messages {
        if let Some(content) = &mut msg.content {
            match content {
                Content::Text(s) => {
                    let (out, f) = tokenize_with_detect(
                        engine,
                        s,
                        policy,
                        detect,
                        light_ner,
                        session.as_deref_mut(),
                        options,
                    )
                    .await?;
                    *s = out;
                    flags.quasi_id_flagged |= f;
                }
                Content::Parts(parts) => {
                    for part in parts {
                        if part.type_ == "text" {
                            if let Some(text) = &mut part.text {
                                let (out, f) = tokenize_with_detect(
                                    engine,
                                    text,
                                    policy,
                                    detect,
                                    light_ner,
                                    session.as_deref_mut(),
                                    options,
                                )
                                .await?;
                                *text = out;
                                flags.quasi_id_flagged |= f;
                            }
                        }
                    }
                }
            }
        }
        if let Some(calls) = &mut msg.tool_calls {
            for call in calls {
                let (out, f) = tokenize_with_detect(
                    engine,
                    &call.function.arguments,
                    policy,
                    detect,
                    light_ner,
                    session.as_deref_mut(),
                    options,
                )
                .await?;
                call.function.arguments = out;
                flags.quasi_id_flagged |= f;
            }
        }
    }
    Ok(flags)
}

/// Tokenise `prompt` (String ou Vec<String>) — aller (legacy).
pub async fn tokenize_completion_request(
    req: &mut CompletionRequest,
    engine: &mut RequestEngine,
    policy: &Policy,
    detect: Option<&DetectClient>,
    light_ner: Option<&LightNer>,
    mut session: Option<&mut SessionContext>,
    options: Option<&SessionOptions>,
) -> Result<SessionFlags, ProxyError> {
    let mut flags = SessionFlags::default();
    match &mut req.prompt {
        Prompt::Single(s) => {
            let (out, f) = tokenize_with_detect(
                engine,
                s,
                policy,
                detect,
                light_ner,
                session.as_deref_mut(),
                options,
            )
            .await?;
            *s = out;
            flags.quasi_id_flagged |= f;
        }
        Prompt::Batch(v) => {
            for s in v {
                let (out, f) = tokenize_with_detect(
                    engine,
                    s,
                    policy,
                    detect,
                    light_ner,
                    session.as_deref_mut(),
                    options,
                )
                .await?;
                *s = out;
                flags.quasi_id_flagged |= f;
            }
        }
    }
    Ok(flags)
}

/// Tokenise un champ texte avec spans sidecar (B.1) — dégradation gracieuse :
/// une erreur du sidecar → warn + détection embarquée seule.
///
/// N0 v1.1 : en présence d'une session (`Some`), tokenisation session (alias
/// et jauge quasi-id) ; sinon comportement historique bit-identique. Renvoie
/// le texte tokenisé et le drapeau de la jauge quasi-id.
///
/// N0 v1.2 (chantier ④) : `light_ner` (NER léger embarqué) produit des spans
/// PERSON/LOC **locaux** fusionnés à ceux du sidecar distant — le core reste
/// la source de vérité (validation stricte + fusion englobante N0).
async fn tokenize_with_detect(
    engine: &mut RequestEngine,
    text: &str,
    policy: &Policy,
    detect: Option<&DetectClient>,
    light_ner: Option<&LightNer>,
    session: Option<&mut SessionContext>,
    options: Option<&SessionOptions>,
) -> Result<(String, bool), ProxyError> {
    let mut extra = match detect {
        Some(client) => match client.detect(text).await {
            Ok(spans) => spans,
            Err(e) => {
                tracing::warn!(detail = %e, "detect sidecar indisponible — détection embarquée seule (dégradation gracieuse)");
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    // N0 v1.2 — NER léger embarqué : spans PERSON/LOC locaux (offsets octets,
    // cohérents avec le contrat interne du core). La dégradation gracieuse est
    // garantie à l'intérieur de `LightNer::detect` (jamais d'erreur).
    if let Some(ner) = light_ner {
        extra.extend(ner.detect(text));
    }
    match (session, options) {
        (Some(sess), Some(opts)) => {
            let r = engine
                .tokenize_session(text, policy, sess, &extra, opts)
                .map_err(proxy_internal)?;
            let flagged = r.quasi_id.as_ref().map(|q| q.flagged).unwrap_or(false);
            Ok((r.text_out, flagged))
        }
        _ => {
            let r = engine
                .tokenize_with_extra(text, policy, &extra)
                .map_err(proxy_internal)?;
            Ok((r.text_out, false))
        }
    }
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
            serde_json::to_value(typed).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to serialize restored response")
                    .with_field("detail", e.to_string())
            })
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
            if let Some(text) = choice
                .get("text")
                .and_then(|t| t.as_str())
                .map(str::to_string)
            {
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
    ProxyError::new(ErrorKind::Internal, "internal tokenization error")
        .with_field("detail", e.to_string())
}

// ---------------------------------------------------------------------------
// STACK-4 — moteur d'audit observe-only
// ---------------------------------------------------------------------------

/// Types à faible cardinalité généralisés par `cloison-core` (jamais
/// tokenisés) : leurs occurrences sont des drapeaux quasi-identifiants.
fn is_quasi_identifier(kind: &DetectorKind) -> bool {
    matches!(
        kind,
        DetectorKind::Ip | DetectorKind::Date | DetectorKind::CreditCard
    )
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
    /// Persistance append-only des reçus (JSONL 0600, `CLOISON_AUDIT_LEDGER_FILE`).
    /// `None` = journal en mémoire seule (perte au restart, dégradé).
    ledger_path: Option<PathBuf>,
    /// Curseur d'ingest vers le contrôle : reçus `[0..cursor]` déjà livrés
    /// (wiring C). `pending_receipts()` = `[cursor..]`.
    ingested_cursor: Mutex<usize>,
    /// Persistance du curseur (fichier `audit_ledger.jsonl.ingested`, 0600) :
    /// après un restart, seuls les reçus jamais ingérés sont re-soumis —
    /// aucune entrée dupliquée dans le journal de transparence.
    ingest_offset_file: Option<PathBuf>,
}

impl AuditEngine {
    /// Construit le moteur : détecteur par défaut, clé chargée ou générée,
    /// hash de politique, seuil k validé (≥ 2).
    ///
    /// `ledger_path` : si fourni, le journal des reçus est **persisté** en
    /// JSONL append-only (mode 0600) et **rechargé** au boot — les reçus
    /// survivent au restart (dette STACK-4). Toujours des compteurs signés,
    /// jamais de texte.
    pub fn new(
        policy: &Policy,
        keys_path: Option<&Path>,
        k: usize,
        ledger_path: Option<&Path>,
    ) -> Result<Self, ProxyError> {
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
        let mut ledger = Vec::new();
        let mut ingest_offset = 0usize;
        if let Some(path) = ledger_path {
            ensure_ledger_file(path)?;
            ledger = load_ledger_file(path)?;
            // Curseur d'ingest durable (wiring C) : fichier `<ledger>.ingested`
            // contenant le nombre de reçus déjà livrés au contrôle.
            let offset_path = ingest_offset_path(path);
            match std::fs::read_to_string(&offset_path) {
                Ok(s) => {
                    ingest_offset = s.trim().parse::<usize>().unwrap_or(0);
                    if ingest_offset > ledger.len() {
                        tracing::warn!(
                            path = %offset_path.display(),
                            cursor = ingest_offset,
                            len = ledger.len(),
                            "curseur d'ingest incohérent — ramené à la longueur du journal"
                        );
                        ingest_offset = ledger.len();
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(
                        path = %offset_path.display(),
                        detail = %e,
                        "lecture du curseur d'ingest impossible — reprise à 0"
                    );
                }
            }
            tracing::info!(
                path = %path.display(),
                reloaded = ledger.len(),
                ingest_cursor = ingest_offset,
                "audit ledger rechargé (JSONL 0600)"
            );
        }
        Ok(Self {
            detector,
            signing_key,
            key_id,
            policy_hash,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            k,
            ledger: Mutex::new(ledger),
            ledger_path: ledger_path.map(Path::to_path_buf),
            ingested_cursor: Mutex::new(ingest_offset),
            ingest_offset_file: ledger_path.map(ingest_offset_path),
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

    /// Accumule un reçu signé dans le journal (pour le rapport) et, si un
    /// chemin de persistance est configuré, l'append au JSONL (0600).
    ///
    /// Une erreur d'écriture est **remontée** (fail-loud) : le reçu reste
    /// disponible en mémoire, mais la perte au restart devient visible.
    pub fn record(&self, receipt: Receipt) -> Result<(), ProxyError> {
        self.ledger
            .lock()
            .expect("audit ledger mutex poisoned")
            .push(receipt.clone());
        if let Some(path) = &self.ledger_path {
            append_receipt_line(path, &receipt)?;
        }
        Ok(())
    }

    /// Nombre de reçus accumulés.
    pub fn receipts_len(&self) -> usize {
        self.ledger
            .lock()
            .expect("audit ledger mutex poisoned")
            .len()
    }

    /// Copie des reçus accumulés (tests, vérification hors-ligne).
    pub fn receipts(&self) -> Vec<Receipt> {
        self.ledger
            .lock()
            .expect("audit ledger mutex poisoned")
            .clone()
    }

    /// Reçus **pendants** — jamais encore livrés au contrôle (wiring C).
    ///
    /// `[ingested_cursor..]` du journal ; vide si tout a été ingéré. Les reçus
    /// restent dans le journal (rapport de conformité + traçabilité) : le
    /// curseur avance seul, rien n'est supprimé (append-only).
    pub fn pending_receipts(&self) -> Vec<Receipt> {
        let ledger = self.ledger.lock().expect("audit ledger mutex poisoned");
        let cursor = self
            .ingested_cursor
            .lock()
            .expect("ingest cursor mutex poisoned")
            .min(ledger.len());
        ledger[cursor..].to_vec()
    }

    /// Marque `n` reçus (les plus anciens pendants) comme ingérés au contrôle.
    ///
    /// Le curseur est **persisté** (fichier `<ledger>.ingested`, 0600, écriture
    /// atomique tmp+rename) quand une persistance est configurée — un restart
    /// ne re-soumet pas les reçus déjà livrés. Une erreur d'écriture est
    /// remontée (fail-loud) : le reçu serait re-soumis au prochain boot (entrée
    /// dupliquée possible, jamais de perte).
    pub fn mark_ingested(&self, n: usize) -> Result<(), ProxyError> {
        if n == 0 {
            return Ok(());
        }
        {
            let ledger = self.ledger.lock().expect("audit ledger mutex poisoned");
            let mut cursor = self
                .ingested_cursor
                .lock()
                .expect("ingest cursor mutex poisoned");
            *cursor = (*cursor + n).min(ledger.len());
        }
        self.persist_ingest_offset()
    }

    /// Persiste le curseur d'ingest (0600, écriture atomique tmp+rename —
    /// jamais de fichier partiellement écrit, invariant I-A10).
    fn persist_ingest_offset(&self) -> Result<(), ProxyError> {
        use std::io::Write;
        let Some(offset_path) = &self.ingest_offset_file else {
            return Ok(());
        };
        let cursor = *self
            .ingested_cursor
            .lock()
            .expect("ingest cursor mutex poisoned");
        let tmp = offset_path.with_extension("tmp");
        {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            crate::fsperm::restrict(0o600).apply(&mut opts);
            let mut file = opts.open(&tmp).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to open ingest offset tmp")
                    .with_field("path", tmp.display().to_string())
                    .with_field("detail", e.to_string())
            })?;
            file.write_all(cursor.to_string().as_bytes())
                .and_then(|_| file.flush())
                .map_err(|e| {
                    ProxyError::new(ErrorKind::Internal, "failed to write ingest offset")
                        .with_field("detail", e.to_string())
                })?;
        }
        std::fs::rename(&tmp, offset_path).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to rename ingest offset file")
                .with_field("detail", e.to_string())
        })
    }

    /// Rapport de conformité k-anonyme sur le journal accumulé.
    ///
    /// Bornes de période : `[min(ts), max(ts)+1)` ; si aucun reçu, `[0, now]`.
    pub fn report(&self) -> Result<ConformanceReport, ProxyError> {
        self.report_for("all")
    }

    /// Rapport de conformité k-anonyme sur la **fenêtre** `period` :
    /// `hourly` = dernière heure, `daily` = dernières 24 h, `weekly` = 7 jours,
    /// `all` = tout le journal (dette STACK-4 : `period` devient filtrant).
    ///
    /// Les bornes du rapport reflètent la fenêtre demandée (`[now-window, now+1)`
    /// pour les fenêtres, `[min(ts), max(ts)+1)` pour `all`).
    pub fn report_for(&self, period: &str) -> Result<ConformanceReport, ProxyError> {
        let window_secs: Option<u64> = match period {
            "hourly" => Some(3600),
            "daily" => Some(86400),
            "weekly" => Some(604800),
            "all" => None,
            other => {
                return Err(ProxyError::new(
                    ErrorKind::BadRequest,
                    "invalid period; expected hourly|daily|weekly|all",
                )
                .with_field("period", other));
            }
        };
        let now = receipt::now_unix();
        let receipts: Vec<Receipt> = match window_secs {
            Some(w) => {
                let start = now.saturating_sub(w);
                self.receipts()
                    .into_iter()
                    .filter(|r| r.ts_unix >= start && r.ts_unix < now + 1)
                    .collect()
            }
            None => self.receipts(),
        };
        let (period_start, period_end) = match window_secs {
            Some(w) => (now.saturating_sub(w), now + 1),
            None => (
                receipts.iter().map(|r| r.ts_unix).min().unwrap_or(0),
                receipts
                    .iter()
                    .map(|r| r.ts_unix)
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(now + 1),
            ),
        };
        let mut report =
            ConformanceReport::from_receipts(&receipts, period_start, period_end, self.k).map_err(
                |e| {
                    ProxyError::new(ErrorKind::Internal, "failed to build conformance report")
                        .with_field("detail", e.to_string())
                },
            )?;
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
                    ProxyError::new(
                        ErrorKind::Internal,
                        "audit key file must be 32 raw bytes or 64 hex chars",
                    )
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
            tracing::warn!(
                "audit mode enabled without CLOISON_AUDIT_KEYS: using an ephemeral signing key"
            );
            Ok(SigningKey::generate(&mut rand::rngs::OsRng))
        }
    }
}

/// Crée le fichier journal des reçus s'il n'existe pas (mode 0600, jamais
/// réécrit s'il existe — append-only).
fn ensure_ledger_file(path: &Path) -> Result<(), ProxyError> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    crate::fsperm::restrict(0o600).apply(&mut opts);
    opts.open(path)
        .map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to create audit ledger file")
                .with_field("path", path.display().to_string())
                .with_field("detail", e.to_string())
        })?;
    Ok(())
}

/// Recharge les reçus persistés (JSONL 0600). Une ligne illisible est
/// signalée (warn) et ignorée — jamais un crash sur un fichier corrompu.
fn load_ledger_file(path: &Path) -> Result<Vec<Receipt>, ProxyError> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to open audit ledger file")
            .with_field("path", path.display().to_string())
            .with_field("detail", e.to_string())
    })?;
    let mut out = Vec::new();
    for (idx, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to read audit ledger line")
                .with_field("path", path.display().to_string())
                .with_field("line", idx.to_string())
                .with_field("detail", e.to_string())
        })?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Receipt>(&line) {
            Ok(r) => out.push(r),
            Err(e) => tracing::warn!(
                path = %path.display(),
                line = idx,
                detail = %e,
                "audit ledger line illisible, ignorée (compteurs uniquement, aucune PII)"
            ),
        }
    }
    Ok(out)
}

/// Append d'un reçu au JSONL : une ligne JSON compacte + flush + fsync
/// (durabilité de la preuve, comme le ledger de transparence).
fn append_receipt_line(path: &Path, receipt: &Receipt) -> Result<(), ProxyError> {
    use std::io::Write;
    let line = serde_json::to_string(receipt).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize audit receipt")
            .with_field("detail", e.to_string())
    })?;
    let mut opts = std::fs::OpenOptions::new();
    opts.append(true);
    crate::fsperm::restrict(0o600).apply(&mut opts);
    let mut file = opts.open(path).map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                "failed to open audit ledger for append",
            )
            .with_field("path", path.display().to_string())
            .with_field("detail", e.to_string())
        })?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            ProxyError::new(
                ErrorKind::Internal,
                "failed to append audit receipt to ledger",
            )
            .with_field("path", path.display().to_string())
            .with_field("detail", e.to_string())
        })
}

/// Écrit une graine 32 octets avec permissions 0600 (jamais de logs dessus).
fn write_seed_file(path: &Path, seed: &[u8; 32]) -> Result<(), ProxyError> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    crate::fsperm::restrict(0o600).apply(&mut opts);
    let mut file = opts.open(path).map_err(|e| {
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

/// Chemin du fichier de curseur d'ingest (`<ledger>.ingested`).
fn ingest_offset_path(ledger_path: &Path) -> PathBuf {
    let mut os = ledger_path.as_os_str().to_os_string();
    os.push(".ingested");
    PathBuf::from(os)
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
