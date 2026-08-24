//! Engine module: orchestrates detection, tokenization, generalization, and restoration.
//!
//! The Engine is the main entry point for CLOISON operations:
//! - `tokenize`: detect PII, generalize low-cardinality, tokenize the rest, replace in text
//! - `restore`: scan for sentinels, verify registry + MAC, restore from vault

use crate::alias::{AliasConfig, AliasExpander, SessionContext};
use crate::detection::{Detector, Span};
use crate::error::{CloisonError, CloisonResult};
use crate::generalize::Generalizer;
use crate::policy::Policy;
use crate::quasi_id::{GaugeConfig, QuasiIdGauge, QuasiIdReport};
use crate::registry::IssuanceRegistry;
use crate::token::{Sentinel, SessionKeys, Token, TokenBody};
use crate::vault::Vault;

/// Reference to an emitted token.
#[derive(Debug, Clone)]
pub struct TokenRef {
    /// Token body base32.
    pub body_b32: String,
    /// Kind tag.
    pub kind_tag: String,
    /// Original clear value.
    pub plain_value: String,
    /// Sentinel string.
    pub sentinel: String,
}

/// Result of a tokenize operation.
#[derive(Debug, Clone)]
pub struct TokenizeResult {
    /// Text with PII replaced by sentinels or generalizations.
    pub text_out: String,
    /// Emitted token references.
    pub emitted: Vec<TokenRef>,
    /// Rapport de la jauge de quasi-identifiants (N0 v1.1, opt-in) ;
    /// `None` si la jauge est désactivée (`SessionOptions`).
    pub quasi_id: Option<QuasiIdReport>,
}

/// Options de la session (alias intra-session + jauge quasi-id) — N0 v1.1.
///
/// Passées séparément de la `Policy` : le serveur (mode non-N0) garde un
/// comportement bit-identique (jamais d'alias/jauge) et `policy_hash`
/// (reçus d'audit) reste inchangé.
#[derive(Debug, Clone, Copy)]
pub struct SessionOptions {
    /// Expansion d'alias intra-session (charte §6.1 couche 4). Défaut : oui
    /// (miroir de la politique du sidecar) — no-op sans mentions en session.
    pub enable_alias_expansion: bool,
    /// Jauge de quasi-identifiants (charte §6.1 couche 6). Défaut : non
    /// (opt-in — signal, jamais de résolution).
    pub enable_quasiid_gauge: bool,
    /// Seuil de la jauge (`score > seuil` strict) ; 1.0 = désactivée de fait.
    pub quasiid_threshold: f64,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            enable_alias_expansion: true,
            enable_quasiid_gauge: false,
            quasiid_threshold: 0.50,
        }
    }
}

/// Counters for restoration operations.
#[derive(Debug, Clone, Default)]
pub struct RestoreCounters {
    /// Successfully restored tokens.
    pub restored: usize,
    /// Incomplete restorations (sentinel found but couldn't restore).
    pub incomplete: usize,
    /// Blocked restorations (MAC mismatch or not in registry).
    pub blocked: usize,
}

/// Result of a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Restored text with clear values.
    pub text_out: String,
    /// Restoration counters.
    pub counters: RestoreCounters,
}

/// The main CLOISON engine.
pub struct Engine {
    /// PII detector.
    detector: Detector,
    /// Encryption vault (optional: may be absent in WASM).
    vault: Option<Vault>,
    /// Session keys.
    keys: SessionKeys,
    /// Generalizer for low-cardinality PII.
    generalizer: Generalizer,
    /// Per-request emission registry.
    registry: IssuanceRegistry,
}

impl Engine {
    /// Create a new engine with the given session keys.
    pub fn new(keys: SessionKeys) -> CloisonResult<Self> {
        let detector = Detector::new()?;
        Ok(Self {
            detector,
            vault: None,
            keys,
            generalizer: Generalizer::new(),
            registry: IssuanceRegistry::new(),
        })
    }

    /// Create a new engine with a vault.
    pub fn with_vault(keys: SessionKeys, vault: Vault) -> CloisonResult<Self> {
        let detector = Detector::new()?;
        Ok(Self {
            detector,
            vault: Some(vault),
            keys,
            generalizer: Generalizer::new(),
            registry: IssuanceRegistry::new(),
        })
    }

    /// Set a custom generalizer.
    pub fn set_generalizer(&mut self, generalizer: Generalizer) {
        self.generalizer = generalizer;
    }

    /// Tokenize text according to the given policy.
    ///
    /// Steps:
    ///   1. Detect all PII matches (filtered by policy)
    ///   2. For each match:
    ///      a. If generalization rule exists → generalize (never tokenize)
    ///      b. Otherwise → emit token, register, store in vault, replace with sentinel
    ///   3. Return tokenized text and emitted token references
    pub fn tokenize(
        &mut self,
        text: &str,
        policy: &Policy,
        request_id: &str,
    ) -> CloisonResult<TokenizeResult> {
        let spans = self.detector.detect_with_policy(text, &policy.detection);
        self.process_spans(text, policy, spans, request_id)
    }

    /// Tokenize with additional NER sidecar spans (wiring edge→detect, B.1).
    ///
    /// Le cœur reste la **source de vérité** : chaque span externe est validé
    /// avant fusion (jamais de confiance aveugle au sidecar) :
    ///   1. type activé par la politique (`policy.detection.is_enabled`) ;
    ///   2. bornes d'octets valides ET sur frontières de caractères UTF-8 ;
    ///   3. valeur **re-tranchée du texte** (`text[start..end]`) — la valeur
    ///      fournie par le sidecar est ignorée ;
    ///   4. aucun chevauchement avec les spans embarqués (ni entre externes).
    ///
    /// Un span externe invalide est ignoré (jamais une erreur — le sidecar
    /// est optionnel et dégradable).
    pub fn tokenize_with_extra(
        &mut self,
        text: &str,
        policy: &Policy,
        request_id: &str,
        extra: &[Span],
    ) -> CloisonResult<TokenizeResult> {
        let mut spans = self.detector.detect_with_policy(text, &policy.detection);
        merge_extra_spans(text, policy, &mut spans, extra);
        let result = self.process_spans(text, policy, spans, request_id)?;
        Ok(result)
    }

    /// Tokenise avec la **session** (N0 v1.1) : alias intra-session (R1–R7)
    /// et jauge de quasi-identifiants, en plus de la détection embarquée et
    /// des spans NER externes validés.
    ///
    /// Étapes :
    ///   1. détection embarquée + spans externes validés (`merge_extra_spans`) ;
    ///   2. **mentions canoniques** : chaque span PERSON/LOC (gazetteer ou
    ///      sidecar) est upserté dans `session` (`seen_count`, borne FIFO) ;
    ///   3. **alias** (si activé) : les formes dérivées (R1–R7) des mentions
    ///      sont matchées dans le texte (jamais les pronoms, scores plafonnés)
    ///      et fusionnées par `AliasExpander::expand` ;
    ///   4. **jauge quasi-id** (si activée) : densité fenêtrée des catégories
    ///      age/act/date/loc — signal, jamais de résolution ;
    ///   5. généralisation/tokenisation (`process_spans` — le core reste la
    ///      source de vérité de la tokenisation).
    ///
    /// Les alias sont tokenisés comme PERSON/LOC (jamais généralisés) ; la
    /// restauration reste bornée au registre de la requête (I3 inchangé).
    pub fn tokenize_session(
        &mut self,
        text: &str,
        policy: &Policy,
        request_id: &str,
        extra: &[Span],
        session: &mut SessionContext,
        options: &SessionOptions,
    ) -> CloisonResult<TokenizeResult> {
        let mut spans = self.detector.detect_with_policy(text, &policy.detection);
        merge_extra_spans(text, policy, &mut spans, extra);

        // 2. Mentions canoniques (PERSON/LOC uniquement — jamais MAIL/TEL/…).
        for s in &spans {
            if let Some(kind) = crate::alias::mention_kind(&s.entity_type) {
                if let Some(value) = text.get(s.start..s.end) {
                    session.upsert(value.to_string(), kind);
                }
            }
        }

        // 3. Expansion d'alias (formes dérivées des mentions déjà connues).
        let spans = if options.enable_alias_expansion && !session.is_empty() {
            let expander = AliasExpander::new(AliasConfig::default());
            expander.expand(text, &spans, session)
        } else {
            spans
        };

        // 4. Jauge de quasi-identifiants (signal, jamais de résolution).
        let quasi_id = if options.enable_quasiid_gauge {
            let gauge = QuasiIdGauge::new(GaugeConfig::default());
            Some(gauge.evaluate(text, &spans, options.quasiid_threshold))
        } else {
            None
        };

        // 5. Généralisation / tokenisation.
        let mut result = self.process_spans(text, policy, spans, request_id)?;
        result.quasi_id = quasi_id;
        Ok(result)
    }

    /// Applique la généralisation/tokenisation sur une liste de spans
    /// (détection embarquée + spans externes validés).
    fn process_spans(
        &mut self,
        text: &str,
        policy: &Policy,
        spans: Vec<Span>,
        _request_id: &str,
    ) -> CloisonResult<TokenizeResult> {
        // Sort by descending position so replacement doesn't break offsets
        let mut spans = spans;
        spans.sort_by_key(|b| std::cmp::Reverse(b.start));

        let mut text_out = text.to_string();
        let mut emitted = Vec::new();

        for span in spans {
            // Step 2a: Check generalization
            if policy.should_generalize(&span.entity_type)
                || self.generalizer.has_rule(&span.entity_type)
            {
                let replacement = self.generalizer.generalize(&span.entity_type, &span.value);
                text_out = replace_span(&text_out, span.start, span.end, &replacement);
                continue;
            }

            // Step 2b: Emit token
            let token = Token::emit(&span.value, &span.entity_type, &self.keys)?;

            // Register in emission registry
            self.registry
                .insert(&token.body, &span.value, &span.entity_type);

            // Store in vault (if available)
            if let Some(ref vault) = self.vault {
                let kind_tag = Sentinel::tag_from_kind(&span.entity_type);
                vault.put(&token.body.to_base32(), &span.value, kind_tag)?;
            }

            let sentinel_str = token.sentinel.format();

            emitted.push(TokenRef {
                body_b32: token.body.to_base32(),
                kind_tag: Sentinel::tag_from_kind(&span.entity_type).to_string(),
                plain_value: span.value.clone(),
                sentinel: sentinel_str.clone(),
            });

            // Replace in text
            text_out = replace_span(&text_out, span.start, span.end, &sentinel_str);
        }

        Ok(TokenizeResult {
            text_out,
            emitted,
            quasi_id: None,
        })
    }

    /// Restore a tokenized text to its clear form.
    ///
    /// Steps:
    ///   1. Scan for sentinel patterns in the text
    ///   2. For each sentinel (reverse order):
    ///      a. Parse the sentinel
    ///      b. Verify the token body is in the emission registry
    ///      c. Retrieve the clear value (registry first, vault fallback)
    ///      d. Verify MAC integrity
    ///      e. Replace sentinel with clear value
    ///   3. Return restored text and counters
    pub fn restore(&self, text: &str, _request_id: &str) -> CloisonResult<RestoreResult> {
        let mut text_out = text.to_string();
        let mut counters = RestoreCounters::default();

        // Step 1: Extract all sentinel positions (scan forward)
        let sentinel_positions = extract_sentinel_positions(&text_out);

        // Step 2: Process in reverse order to preserve offsets
        for (sentinel_str, start, end) in sentinel_positions.into_iter().rev() {
            let parsed = match Sentinel::parse(&sentinel_str) {
                Some(s) => s,
                None => {
                    counters.blocked += 1;
                    continue;
                }
            };

            let body = match TokenBody::from_base32(&parsed.token_body_b32) {
                Ok(b) => b,
                Err(_) => {
                    counters.blocked += 1;
                    continue;
                }
            };

            let kind = match Sentinel::kind_from_tag(&parsed.kind_tag) {
                Ok(k) => k,
                Err(_) => {
                    counters.blocked += 1;
                    continue;
                }
            };

            // Step 2b: Verify emission registry
            if !self.registry.contains(&body) {
                counters.blocked += 1;
                continue;
            }

            // Step 2c: Retrieve clear value
            let plain_value = if let Some((val, _kind)) = self.registry.get(&body) {
                val.clone()
            } else if let Some(ref vault) = self.vault {
                match vault.get(&parsed.token_body_b32) {
                    Ok(Some((val, _tag))) => val,
                    Ok(None) => {
                        counters.incomplete += 1;
                        continue;
                    }
                    Err(CloisonError::VaultTtlExpired(_)) => {
                        counters.incomplete += 1;
                        continue;
                    }
                    Err(_) => {
                        counters.incomplete += 1;
                        continue;
                    }
                }
            } else {
                counters.incomplete += 1;
                continue;
            };

            // Step 2d: Verify MAC integrity
            if !Token::verify_body(&body, &plain_value, &kind, &self.keys) {
                counters.blocked += 1;
                continue;
            }

            // Step 2e: Replace sentinel with clear value
            text_out = replace_span(&text_out, start, end, &plain_value);
            counters.restored += 1;
        }

        Ok(RestoreResult { text_out, counters })
    }

    /// Clear the emission registry (call at end of request).
    pub fn clear_registry(&mut self) {
        self.registry.clear();
    }

    /// Get the current emission registry.
    pub fn registry(&self) -> &IssuanceRegistry {
        &self.registry
    }

    /// Get the session keys.
    pub fn keys(&self) -> &SessionKeys {
        &self.keys
    }

    /// Get the detector.
    pub fn detector(&self) -> &Detector {
        &self.detector
    }

    /// Run detection only (no tokenization).
    pub fn detect(&self, text: &str, policy: &Policy) -> Vec<Span> {
        self.detector.detect_with_policy(text, &policy.detection)
    }
}

/// Valide et fusionne les spans NER externes (wiring edge→detect, B.1) dans
/// la liste des spans embarqués. Le cœur reste la **source de vérité** :
/// chaque span externe est validé avant fusion (jamais de confiance aveugle
/// au sidecar) :
///   1. type activé par la politique (`policy.detection.is_enabled`) ;
///   2. bornes d'octets valides ET sur frontières de caractères UTF-8 ;
///   3. valeur **re-tranchée du texte** (`text[start..end]`) — la valeur
///      fournie par le sidecar est ignorée ;
///   4. aucun chevauchement avec les spans embarqués (ni entre externes).
///
/// Un span externe invalide est ignoré (jamais une erreur — le sidecar
/// est optionnel et dégradable).
fn merge_extra_spans(text: &str, policy: &Policy, spans: &mut Vec<Span>, extra: &[Span]) {
    for s in extra {
        if !policy.detection.is_enabled(&s.entity_type) {
            continue;
        }
        // Bornes valides + frontières UTF-8 (les offsets sidecar sont des
        // octets — un mauvais découpage ne doit jamais paniquer).
        if s.start >= s.end || s.end > text.len() {
            continue;
        }
        if !text.is_char_boundary(s.start) || !text.is_char_boundary(s.end) {
            continue;
        }
        // Pas de chevauchement avec un span déjà retenu (core ou externe).
        if spans.iter().any(|c| s.start < c.end && c.start < s.end) {
            continue;
        }
        spans.push(Span {
            entity_type: s.entity_type.clone(),
            start: s.start,
            end: s.end,
            score: s.score,
            value: text[s.start..s.end].to_string(),
        });
    }
}

/// Replace a span [start, end) in text with a replacement string.
fn replace_span(text: &str, start: usize, end: usize, replacement: &str) -> String {
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    result
}

/// Extract all sentinel positions from text.
/// Returns (sentinel_string, start, end) tuples in forward order.
fn extract_sentinel_positions(text: &str) -> Vec<(String, usize, usize)> {
    let mut positions = Vec::new();
    let open = Sentinel::L_OPEN;
    let close = Sentinel::L_CLOSE;
    let open_str = open.to_string();
    let close_str = close.to_string();

    let mut search_start = 0;
    while search_start < text.len() {
        // Find opening delimiter
        let Some(open_pos) = text[search_start..].find(&open_str) else {
            break;
        };
        let abs_open = search_start + open_pos;

        // Find closing delimiter after the opening
        let after_open = abs_open + open.len_utf8();
        let Some(close_pos) = text[after_open..].find(&close_str) else {
            // Ouverture non fermee : sentinelle tronquee (ex. coupure max_tokens).
            // On signale le fragment : le moteur de restauration le remplacera
            // par le marqueur neutre (fail-loud) et incrementera `incomplete`.
            // Jamais de jeton brut transmis.
            positions.push((text[abs_open..].to_string(), abs_open, text.len()));
            break;
        };
        let abs_close = after_open + close_pos + close.len_utf8();

        let sentinel_str = text[abs_open..abs_close].to_string();

        // Validate that this looks like a sentinel
        if let Some(_parsed) = Sentinel::parse(&sentinel_str) {
            positions.push((sentinel_str, abs_open, abs_close));
        }

        search_start = abs_close;
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::DetectorKind;
    use crate::policy::Policy;
    use crate::token::SessionKeys;

    fn test_keys() -> SessionKeys {
        SessionKeys::derive([0xABu8; 32], [0xCDu8; 16]).unwrap()
    }

    #[test]
    fn test_tokenize_restore_roundtrip() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let original = "Contact: user@example.com ou +221 77 123 45 67";
        let result = engine.tokenize(original, &policy, "req-1").unwrap();

        // Text should not contain the original PII values
        assert!(!result.text_out.contains("user@example.com"));

        // Restore
        let restored = engine.restore(&result.text_out, "req-1").unwrap();
        assert_eq!(restored.text_out, original);
        assert!(restored.counters.blocked == 0);
    }

    #[test]
    fn test_restore_canonicalized_value() {
        // Régression : le MAC est calculé sur la valeur canonique ("Aminata" ->
        // "aminata"). La restauration doit accepter la valeur brute du
        // registre (capitalisée), sinon toute valeur non-canonique échoue.
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let original = "Nom: Aminata Diop, tel +221 77 123 45 67";
        let result = engine.tokenize(original, &policy, "req-cap").unwrap();
        assert!(!result.text_out.contains("Aminata"));

        let restored = engine.restore(&result.text_out, "req-cap").unwrap();
        assert_eq!(
            restored.text_out, original,
            "nom capitalisé + téléphone restaurés"
        );
        assert_eq!(restored.counters.blocked, 0, "aucun jeton bloqué");
        assert_eq!(restored.counters.restored, 2, "nom + téléphone restaurés");
    }

    #[test]
    fn test_no_clear_leaving() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let original = "Email: test@test.com";
        let result = engine.tokenize(original, &policy, "req-2").unwrap();
        assert!(!result.text_out.contains("test@test.com"));
    }

    #[test]
    fn test_blocked_fake_sentinel() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        // First tokenize something to set up the registry
        let _ = engine
            .tokenize("Contact: user@example.com", &policy, "req-3")
            .unwrap();

        // Now try to restore text with a forged sentinel
        let fake_sentinel = format!(
            "{}{}{}{}{}",
            Sentinel::L_OPEN,
            "AAAAAAAAAAAAAAAAAAAAAAAAAA",
            Sentinel::L_SEP,
            "EM",
            Sentinel::L_CLOSE
        );

        let result = engine.restore(&fake_sentinel, "req-3").unwrap();
        assert!(
            result.counters.blocked > 0,
            "Forged sentinel should be blocked"
        );
    }

    #[test]
    fn test_determinism_same_session() {
        let keys = test_keys();
        let mut engine1 = Engine::new(keys.clone()).unwrap();
        let mut engine2 = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let r1 = engine1
            .tokenize("user@example.com", &policy, "req-a")
            .unwrap();
        let r2 = engine2
            .tokenize("user@example.com", &policy, "req-b")
            .unwrap();

        assert_eq!(r1.emitted[0].body_b32, r2.emitted[0].body_b32);
    }

    #[test]
    fn test_rotation_different_session() {
        let keys1 = SessionKeys::derive([0xABu8; 32], [0x01u8; 16]).unwrap();
        let keys2 = SessionKeys::derive([0xABu8; 32], [0x02u8; 16]).unwrap();
        let mut engine1 = Engine::new(keys1).unwrap();
        let mut engine2 = Engine::new(keys2).unwrap();
        let policy = Policy::default();

        let r1 = engine1
            .tokenize("user@example.com", &policy, "req-a")
            .unwrap();
        let r2 = engine2
            .tokenize("user@example.com", &policy, "req-b")
            .unwrap();

        assert_ne!(r1.emitted[0].body_b32, r2.emitted[0].body_b32);
    }

    // -----------------------------------------------------------------------
    // Wiring edge→detect (B.1) : spans NER externes validés par le cœur.
    // -----------------------------------------------------------------------

    /// Span NER externe (PERSON) — offsets d'octets de "Xolani Ndlovu".
    fn person_span(text: &str) -> Span {
        let start = text.find("Xolani Ndlovu").expect("nom présent");
        Span {
            entity_type: DetectorKind::Person,
            start,
            end: start + "Xolani Ndlovu".len(),
            score: 0.95,
            value: String::new(), // la valeur sidecar est IGNORÉE (re-tranchée)
        }
    }

    #[test]
    fn test_tokenize_with_extra_person_masked_and_roundtrip() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        // Nom NON sénégalais : aucun détecteur embarqué ne le trouve — seul
        // le span externe (sidecar NER) permet le masquage.
        let text = "Xolani Ndlovu vient de Soweto";
        let extra = vec![person_span(text)];
        let result = engine
            .tokenize_with_extra(text, &policy, "req-extra-1", &extra)
            .unwrap();

        assert!(
            !result.text_out.contains("Xolani Ndlovu"),
            "nom masqué par le span externe"
        );
        assert!(result.text_out.contains('⟦'), "sentinelle émise");
        assert_eq!(result.emitted.len(), 1);
        assert_eq!(result.emitted[0].kind_tag, "PE", "tag PERSON");

        // Roundtrip complet (restauration des jetons de cette requête).
        let restored = engine.restore(&result.text_out, "req-extra-1").unwrap();
        assert_eq!(restored.text_out, text);
        assert_eq!(restored.counters.blocked, 0);
        assert_eq!(restored.counters.restored, 1);
    }

    #[test]
    fn test_tokenize_with_extra_invalid_spans_ignored() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let text = "Xolani Ndlovu vient de Soweto";
        let n = text.len();

        // 1. Hors bornes (end dépasse la longueur du texte).
        let out_of_bounds = Span {
            entity_type: DetectorKind::Person,
            start: n - 2,
            end: n + 5,
            score: 0.9,
            value: String::new(),
        };
        // 2. Type non activé par la politique (gazetteer inconnu).
        let disabled = Span {
            entity_type: DetectorKind::Gazetteer("inexistant_gz".to_string()),
            start: 0,
            end: 6,
            score: 0.9,
            value: String::new(),
        };

        let extra = vec![out_of_bounds, disabled];
        let result = engine
            .tokenize_with_extra(text, &policy, "req-extra-2", &extra)
            .unwrap();
        // Aucun span externe valide → texte intact, aucun jeton émis.
        assert_eq!(result.text_out, text, "spans invalides ignorés");
        assert!(result.emitted.is_empty());

        // 3. Frontière UTF-8 invalide (start au milieu d'un caractère multi-octets).
        let text_utf8 = "é Xolani Ndlovu";
        let bad_boundary = Span {
            entity_type: DetectorKind::Person,
            start: 1, // au milieu de 'é' (2 octets)
            end: 1 + "Xolani Ndlovu".len(),
            score: 0.9,
            value: String::new(),
        };
        let r2 = engine
            .tokenize_with_extra(text_utf8, &policy, "req-extra-3", &[bad_boundary])
            .unwrap();
        assert_eq!(r2.text_out, text_utf8, "frontière UTF-8 invalide rejetée");
    }

    #[test]
    fn test_tokenize_with_extra_overlap_core_span_skipped() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        // L'email est détecté par le core ; un span externe PERSON qui
        // recouvre l'email doit être ignoré (le core prime).
        let text = "Contactez user@example.com svp";
        let start = text.find("user@example.com").unwrap();
        let overlapping = Span {
            entity_type: DetectorKind::Person,
            start,
            end: start + "user@example.com".len(),
            score: 0.9,
            value: String::new(),
        };
        let result = engine
            .tokenize_with_extra(text, &policy, "req-extra-4", &[overlapping])
            .unwrap();
        assert!(
            !result.text_out.contains("user@example.com"),
            "email masqué par le core"
        );
        assert_eq!(
            result.emitted.len(),
            1,
            "un seul jeton (l'email), pas de doublon"
        );
        assert_eq!(result.emitted[0].kind_tag, "EM");
    }

    // -----------------------------------------------------------------------
    // N0 v1.1 — alias intra-session + jauge quasi-id in-core
    // -----------------------------------------------------------------------

    #[test]
    fn test_tokenize_session_alias_masks_derived_form() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();
        let options = SessionOptions::default();
        let mut session = SessionContext::new();

        // Message 1 : la mention canonique « Mamadou » (gazetteer) est
        // détectée et enregistrée dans la session.
        let r1 = engine
            .tokenize_session(
                "Mamadou est arrivé.",
                &policy,
                "req-s1",
                &[],
                &mut session,
                &options,
            )
            .unwrap();
        assert!(r1.text_out.contains('⟦'), "Mamadou masqué au msg 1");
        assert_eq!(session.mentions.len(), 1);
        assert_eq!(session.mentions[0].key, "Mamadou");

        // Message 2 : « Momo » (diminutif R5) n'est dans aucun gazetteer —
        // seul l'alias intra-session le masque.
        let r2 = engine
            .tokenize_session(
                "Momo aussi.",
                &policy,
                "req-s2",
                &[],
                &mut session,
                &options,
            )
            .unwrap();
        assert!(
            r2.text_out.contains('⟦'),
            "diminutif masqué par alias : {}",
            r2.text_out
        );
        assert!(
            !r2.text_out.contains("Momo"),
            "aucun clair : {}",
            r2.text_out
        );

        // Roundtrip : la restauration rend « Momo » (jeton de cette requête).
        let restored = engine.restore(&r2.text_out, "req-s2").unwrap();
        assert_eq!(restored.text_out, "Momo aussi.");
        assert_eq!(restored.counters.blocked, 0);
    }

    #[test]
    fn test_tokenize_session_pronoun_never_masked() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();
        let options = SessionOptions::default();
        let mut session = SessionContext::new();
        session.upsert("Marie Dupont".to_string(), DetectorKind::Person);

        let r = engine
            .tokenize_session(
                "il est parti. elle aussi.",
                &policy,
                "req-pron",
                &[],
                &mut session,
                &options,
            )
            .unwrap();
        assert_eq!(
            r.text_out, "il est parti. elle aussi.",
            "les pronoms ne sont JAMAIS masqués"
        );
        assert!(r.emitted.is_empty());
    }

    #[test]
    fn test_tokenize_session_alias_disabled_noop() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();
        let options = SessionOptions {
            enable_alias_expansion: false,
            ..Default::default()
        };
        let mut session = SessionContext::new();
        session.upsert("Mamadou".to_string(), DetectorKind::Person);

        // Alias désactivé : « Momo » n'est pas dans un gazetteer → clair.
        let r = engine
            .tokenize_session(
                "Momo est là.",
                &policy,
                "req-noalias",
                &[],
                &mut session,
                &options,
            )
            .unwrap();
        assert_eq!(r.text_out, "Momo est là.", "alias désactivé → no-op");
        assert!(r.emitted.is_empty());
    }

    #[test]
    fn test_tokenize_session_quasi_id_flagged() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();
        let options = SessionOptions {
            enable_quasiid_gauge: true,
            quasiid_threshold: 0.5,
            ..Default::default()
        };
        let mut session = SessionContext::new();

        let text = "Il a 42 ans, acte n° 1847, enregistré le 12/03/2021 à Dakar.";
        let r = engine
            .tokenize_session(text, &policy, "req-qid", &[], &mut session, &options)
            .unwrap();
        let report = r.quasi_id.expect("jauge activée → rapport présent");
        assert!(report.flagged, "densité age+act+date+loc → flag");
        assert_eq!(
            report.signals.len(),
            4,
            "les 4 catégories présentes: {:?}",
            report.signals
        );

        // Jauge désactivée → aucun rapport, aucun changement de sortie.
        let mut options_off = options;
        options_off.enable_quasiid_gauge = false;
        let r2 = engine
            .tokenize_session(text, &policy, "req-qid2", &[], &mut session, &options_off)
            .unwrap();
        assert!(r2.quasi_id.is_none());
    }

    #[test]
    fn test_tokenize_session_server_mode_unchanged() {
        // Hors session (serveur) : tokenize_with_extra ne touche ni à la
        // session ni à la jauge — comportement bit-identique au serveur.
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();
        let text = "Contact: user@example.com, tel +221 77 123 45 67";
        let r = engine
            .tokenize_with_extra(text, &policy, "req-srv", &[])
            .unwrap();
        assert!(r.quasi_id.is_none());
        let restored = engine.restore(&r.text_out, "req-srv").unwrap();
        assert_eq!(restored.text_out, text);
    }
}
