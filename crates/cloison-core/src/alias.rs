//! Expansion d'alias intra-session (charte §6.1 couche 4) — IN-CORE (N0 v1.1).
//!
//! Portage Rust des règles R1–R7 du sidecar (`services/cloison-detect/src/
//! alias.py`, référence de règles) : prénom seul (R1), titre + nom (R2),
//! nom seul hors noms communs (R3), prénom + initiale (R4, off par défaut),
//! diminutifs (R5), raccourcis de lieux (R6), casse/diacritiques au matching
//! (R7). ZÉRO réécriture de logique : les sémantiques (scores plafonnés,
//! déduplication, ordre déterministe) sont identiques au sidecar.
//!
//! Invariants de sécurité :
//! - **Jamais les pronoms** : pronoms et mots-outils ne sont JAMAIS dérivés
//!   ni matchés (pas de fuite — charte §6.1 couche 4 : « le pronom "elle"
//!   n'est pas une fuite »).
//! - **Score plafonné** : alias ≤ `score_cap × score canonique` (boost borné
//!   par `max_score` et par le canonique) — jamais un score gonflé.
//! - **Aucune inférence hors contexte** : session vide → no-op.
//! - **Déterminisme** : mêmes entrées → mêmes sorties (tri + ordre stable).
//!
//! L'état de session (mentions canoniques) est détenu par le *propriétaire*
//! (`SessionContext` — le daemon N0 côté proxy) ; l'expandeur est stateless
//! (comme le sidecar : « le core reste propriétaire du store de mentions »).

use std::collections::{BTreeMap, HashMap, HashSet};

use regex::Regex;

use crate::detection::{DetectorKind, Span, GAZETTEER_NOM_SN, GAZETTEER_VILLE_SN};
use crate::round4;

// ---------------------------------------------------------------------------
// Normalisation texte (règle R7) — miroir de `normalize_text` du sidecar
// ---------------------------------------------------------------------------

/// Normalise casse + diacritiques : minuscules, accents retirés (NFKD).
///
/// « OUAGADOUGOU » / « Ouagadougou » / « Oùagàdougou » → « ouagadougou ».
/// N'affecte jamais les offsets : utilisée pour l'index d'alias et le
/// matching, jamais pour produire des spans.
pub fn normalize_text(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    value
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Classes accent-insensibles par base (miroir de `_ACCENTED_BY_BASE`).
const ACCENTED_BY_BASE: &[(&str, &[char])] = &[
    ("a", &['à', 'â', 'ä', 'á', 'ã', 'å']),
    ("c", &['ç']),
    ("e", &['é', 'è', 'ê', 'ë']),
    ("i", &['î', 'ï', 'í', 'ì']),
    ("n", &['ñ']),
    ("o", &['ô', 'ö', 'ò', 'ó', 'õ']),
    ("u", &['ù', 'û', 'ü', 'ú']),
    ("y", &['ÿ']),
];

/// Construit une classe regex insensible casse + diacritiques pour un mot
/// (miroir de `insensitive_pattern` du sidecar) :
/// « Aïcha » → `[AaÀÂÄÁÃÅ][IiÎÏÍÌ]...` — réutilisée par l'index d'alias.
pub fn insensitive_pattern(word: &str) -> String {
    let mut out = String::new();
    for ch in word.chars() {
        if !ch.is_alphanumeric() {
            out.push_str(&regex::escape(&ch.to_string()));
            continue;
        }
        use unicode_normalization::UnicodeNormalization;
        let base = ch
            .nfkd()
            .find(|c| !unicode_normalization::char::is_combining_mark(*c))
            .unwrap_or(ch);
        let lower = base.to_ascii_lowercase();
        let mut variants: Vec<char> = vec![base, lower, base.to_ascii_uppercase()];
        let lower_s = lower.to_string();
        let extras: &[char] = ACCENTED_BY_BASE
            .iter()
            .find(|(b, _)| **b == *lower_s)
            .map(|(_, xs)| *xs)
            .unwrap_or(&[]);
        for extra in extras {
            variants.push(*extra);
            // `to_uppercase` (unicode) — `to_ascii_uppercase` n'uppercase pas
            // les accents (« ï » → « I » au lieu de « Ï », miroir du `upper()`
            // Python).
            if let Some(u) = extra.to_uppercase().next() {
                variants.push(u);
            }
        }
        variants.sort_unstable();
        variants.dedup();
        if variants.len() > 1 {
            out.push('[');
            for v in variants {
                out.push(v);
            }
            out.push(']');
        } else {
            out.push_str(&regex::escape(&base.to_string()));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Formes pronominales / mots-outils : JAMAIS traitées comme des fuites
// ---------------------------------------------------------------------------

/// Formes pronominales / mots-outils (miroir de `PRONOUN_FORMS` du sidecar).
pub const PRONOUN_FORMS: &[&str] = &[
    "il", "elle", "ils", "elles", "on", "nous", "vous", "tu", "te", "toi", "je", "j", "moi", "lui",
    "leur", "leurs", "le", "la", "les", "y", "en", "ce", "cet", "cette", "ces", "un", "une", "des",
    "du", "de", "d", "l", "qui", "que", "quoi", "dont", "ou", "où",
];

/// Noms trop communs — jamais dérivés ni matchés (R3 ; miroir de
/// `_DEFAULT_COMMON_NAMES` du sidecar).
pub const DEFAULT_COMMON_NAMES: &[&str] = &[
    "les",
    "le",
    "la",
    "de",
    "du",
    "des",
    "et",
    "un",
    "une",
    "au",
    "aux",
    "madame",
    "monsieur",
    "mme",
    "mlle",
    "dr",
    "pr",
    "m",
    "a",
    "à",
    "ce",
    "cette",
    "ces",
    "son",
    "sa",
    "ses",
    "mon",
    "ma",
    "mes",
    "pour",
    "par",
    "sur",
    "dans",
    "avec",
    "sans",
    "chez",
    "entre",
    "sous",
    "vers",
    "depuis",
    "pendant",
    "avant",
    "après",
    "tous",
    "tout",
    "toute",
    "toutes",
    "chaque",
    "quelque",
    "autres",
    "autre",
    "rue",
    "avenue",
    "secteur",
    "quartier",
    "ville",
    "commune",
    "province",
    "région",
    "region",
    "departement",
    "département",
    "cité",
    "cite",
    "non",
    "oui",
    "bonjour",
    "merci",
    "ministère",
    "ministere",
    "préfecture",
    "prefecture",
    "mairie",
    "hôpital",
    "hopital",
    "école",
    "ecole",
    "marché",
    "marche",
    "place",
    "centre",
    "camp",
    "village",
    "terrain",
    "parcelle",
    "lot",
    "numero",
    "numéro",
    "n",
    "n°",
];

/// Titres dérivables (R2 — miroir de `AliasConfig.titles` du sidecar).
pub const DEFAULT_TITLES: &[&str] = &["M.", "Mme", "Mlle", "Dr", "Pr", "Madame", "Monsieur"];

/// Diminutifs par défaut (R5 — miroir du sidecar) : « Momo » → « Mamadou ».
pub const DEFAULT_DIMINUTIVES: &[(&str, &str)] = &[("Momo", "Mamadou")];

/// Raccourcis de lieux par défaut (R6 — miroir du sidecar) :
/// « Ouaga » → « Ouagadougou ».
pub const DEFAULT_PLACE_SHORTCUTS: &[(&str, &str)] = &[("Ouaga", "Ouagadougou")];

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Règles d'alias (miroir de `AliasConfig` du sidecar). Valeurs par défaut
/// identiques ; la seule différence : structures Rust (vecteurs au lieu de
/// tuples/dicts — même contenu, même ordre de priorité).
#[derive(Debug, Clone)]
pub struct AliasConfig {
    /// Titres dérivables (R2).
    pub titles: Vec<String>,
    /// Diminutifs (R5) : (diminutif, forme canonique).
    pub diminutives: Vec<(String, String)>,
    /// Raccourcis de lieux (R6) : (raccourci, forme longue).
    pub place_shortcuts: Vec<(String, String)>,
    /// Noms trop communs (R3) : jamais dérivés ni matchés.
    pub common_names: Vec<String>,
    /// Formes pronominales / mots-outils : jamais de fuite.
    pub pronouns: Vec<String>,
    /// Garde-fou anti-explosion : nombre maximal de formes dérivées.
    pub max_derived_forms: usize,
    /// R4 (prénom + initiale) : off par défaut.
    pub enable_initial_forms: bool,
    /// Un alias ne dépasse jamais `score_cap × score canonique`.
    pub score_cap: f64,
    /// Plafond absolu du score d'alias (le `seen_count` ne le dépasse jamais).
    pub max_score: f64,
    /// Score canonique par défaut quand la mention n'est pas spanée ici.
    pub default_canonical_score: f64,
}

impl Default for AliasConfig {
    fn default() -> Self {
        Self {
            titles: DEFAULT_TITLES.iter().map(|s| s.to_string()).collect(),
            diminutives: DEFAULT_DIMINUTIVES
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            place_shortcuts: DEFAULT_PLACE_SHORTCUTS
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            common_names: DEFAULT_COMMON_NAMES.iter().map(|s| s.to_string()).collect(),
            pronouns: PRONOUN_FORMS.iter().map(|s| s.to_string()).collect(),
            max_derived_forms: 8,
            enable_initial_forms: false,
            score_cap: 0.85,
            max_score: 0.95,
            default_canonical_score: 0.80,
        }
    }
}

// ---------------------------------------------------------------------------
// Mentions canoniques & contexte de session
// ---------------------------------------------------------------------------

/// Mention canonique établie dans la session (miroir de `CanonicalMention`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMention {
    /// Clé canonique : le texte exact de la mention détectée.
    pub key: String,
    /// Type de la mention — **Person** ou **Location** uniquement (les alias
    /// ne s'appliquent qu'à PERSON/LOC, charte §6.1 couche 4).
    pub kind: DetectorKind,
    /// Nombre d'occurrences dans la session (boost borné du score d'alias).
    pub seen_count: u32,
}

/// Contexte de session : mentions canoniques accumulées (détenu par le
/// daemon N0 — l'expandeur est stateless). Borne documentaire
/// `max_mentions` (défaut 200, miroir de `session_mentions_max`).
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// Mentions canoniques de la session, dans l'ordre d'apparition.
    pub mentions: Vec<CanonicalMention>,
    /// Borne du nombre de mentions (FIFO au-delà).
    pub max_mentions: usize,
}

impl SessionContext {
    /// Nouvelle session vide (borne par défaut : 200 mentions).
    pub fn new() -> Self {
        Self {
            mentions: Vec::new(),
            max_mentions: 200,
        }
    }

    /// Session avec une borne explicite.
    pub fn with_max(max_mentions: usize) -> Self {
        Self {
            mentions: Vec::new(),
            max_mentions,
        }
    }

    /// Session vide ?
    pub fn is_empty(&self) -> bool {
        self.mentions.is_empty()
    }

    /// Vide la session (rotation explicite du daemon).
    pub fn clear(&mut self) {
        self.mentions.clear();
    }

    /// Enregistre une mention canonique : même clé (normalisée) + même type →
    /// `seen_count` incrémenté ; sinon insertion (FIFO au-delà de la borne —
    /// la borne est documentaire, jamais une fuite).
    pub fn upsert(&mut self, key: String, kind: DetectorKind) {
        let norm = normalize_text(&key);
        if let Some(m) = self
            .mentions
            .iter_mut()
            .find(|m| m.kind == kind && normalize_text(&m.key) == norm)
        {
            m.seen_count = m.seen_count.saturating_add(1);
            return;
        }
        if self.mentions.len() >= self.max_mentions {
            self.mentions.remove(0);
        }
        self.mentions.push(CanonicalMention {
            key,
            kind,
            seen_count: 1,
        });
    }
}

/// Type de mention exploitable par l'alias pour un type de span :
/// PERSON/gazetteer `nom_sn` → Person ; LOCATION/gazetteer `ville_sn` →
/// Location ; tout autre type → `None` (jamais d'alias sur MAIL/TEL/CNI/…).
pub fn mention_kind(kind: &DetectorKind) -> Option<DetectorKind> {
    match kind {
        DetectorKind::Person => Some(DetectorKind::Person),
        DetectorKind::Location => Some(DetectorKind::Location),
        DetectorKind::Gazetteer(n) if n == GAZETTEER_NOM_SN => Some(DetectorKind::Person),
        DetectorKind::Gazetteer(n) if n == GAZETTEER_VILLE_SN => Some(DetectorKind::Location),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Expandeur d'alias (stateless par requête — l'index est reconstruit à
// chaque appel, comme le sidecar)
// ---------------------------------------------------------------------------

/// Expandeur d'alias : dérive (R1–R7) et matche les formes dérivées des
/// mentions canoniques de la session dans un texte.
pub struct AliasExpander {
    config: AliasConfig,
    /// Formes bloquées (noms communs + pronoms), normalisées : jamais
    /// dérivées ni matchées.
    blocked_norm: HashSet<String>,
    /// Titres normalisés : jamais traités comme un prénom (R1).
    title_norms: HashSet<String>,
}

impl AliasExpander {
    /// Construit l'expandeur avec une configuration.
    pub fn new(config: AliasConfig) -> Self {
        let blocked_norm: HashSet<String> = config
            .common_names
            .iter()
            .chain(config.pronouns.iter())
            .map(|s| normalize_text(s))
            .collect();
        let title_norms: HashSet<String> =
            config.titles.iter().map(|s| normalize_text(s)).collect();
        Self {
            config,
            blocked_norm,
            title_norms,
        }
    }

    /// Formes dérivées (R1–R7) pour une mention canonique, triées et
    /// bornées par `max_derived_forms`. R7 est appliquée au matching.
    pub fn derive(&self, m: &CanonicalMention) -> Vec<String> {
        let key = m.key.trim();
        if key.is_empty() {
            return Vec::new();
        }
        let tokens: Vec<&str> = key.split_whitespace().collect();
        if tokens.is_empty() {
            return Vec::new();
        }
        let norm_key = normalize_text(key);
        if norm_key.is_empty() || self.blocked_norm.contains(&norm_key) {
            // un pronom / mot banal n'est jamais une mention exploitable
            return Vec::new();
        }

        let mut forms: Vec<String> = Vec::new();
        let first = tokens[0];
        let last = tokens[tokens.len() - 1];

        if tokens.len() >= 2 {
            // R1 — prénom seul (jamais un titre, jamais un mot banal)
            let norm_first = normalize_text(first);
            if first.chars().count() >= 2
                && !self.blocked_norm.contains(&norm_first)
                && !self.title_norms.contains(&norm_first)
            {
                forms.push(first.to_string());
            }
            // R2 — titre + nom
            if last.chars().count() >= 2 {
                for title in &self.config.titles {
                    forms.push(format!("{title} {last}"));
                }
            }
            // R3 — nom seul (hors noms communs / pronoms)
            if last.chars().count() >= 2 && !self.blocked_norm.contains(&normalize_text(last)) {
                forms.push(last.to_string());
            }
            // R4 — prénom + initiale (optionnel, off par défaut)
            if self.config.enable_initial_forms {
                let first_initial = first.chars().next().unwrap_or(' ');
                let last_initial = last.chars().next().unwrap_or(' ');
                forms.push(format!("{first} {last_initial}."));
                forms.push(format!("{first_initial}. {last_initial}."));
            }
        }
        // R5 — diminutifs
        for (dim, canon) in &self.config.diminutives {
            if normalize_text(canon) == norm_key {
                forms.push(dim.clone());
            }
        }
        // R6 — raccourcis de lieux
        for (short, long) in &self.config.place_shortcuts {
            if normalize_text(long) == norm_key {
                forms.push(short.clone());
            }
        }

        let mut clean: Vec<String> = forms
            .into_iter()
            .filter(|f| {
                let nf = normalize_text(f);
                !f.trim().is_empty()
                    && nf.chars().count() >= 2
                    && !self.blocked_norm.contains(&nf)
                    && nf != norm_key
            })
            .collect();
        clean.sort_unstable();
        clean.dedup();
        if clean.len() > self.config.max_derived_forms {
            clean.truncate(self.config.max_derived_forms);
        }
        clean
    }

    /// Enrichit `spans` des alias (source `alias`, `alias_of` = clé
    /// canonique). Session vide ou texte vide → spans inchangés. Chaque
    /// alias est dédupliqué contre les spans existants : couvert → ignoré ;
    /// englobe strictement → extension ; chevauchement partiel → l'existant
    /// fait foi (miroir de `_merge_alias` du sidecar).
    pub fn expand(&self, text: &str, spans: &[Span], session: &SessionContext) -> Vec<Span> {
        if session.mentions.is_empty() || text.is_empty() {
            return spans.to_vec();
        }
        let index = self.build_index(session);
        let canonical_scores = canonical_scores(text, spans, session);
        let existing: Vec<Span> = spans.to_vec();
        let mut working: Vec<Span> = spans.to_vec();

        // Formes par longueur décroissante puis ordre lexicographique
        // (déterministe — miroir de `sorted(index, key=(-len, f))`).
        let mut norm_forms: Vec<&String> = index.keys().collect();
        norm_forms.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));

        for norm_form in norm_forms {
            let entries = &index[norm_form];
            // Une seule regex par forme (la première forme originale suffit).
            // La crate `regex` n'a pas de lookaround : la frontière de mot est
            // vérifiée manuellement autour de chaque correspondance (équivalent
            // exact de `(?<![\w-])…(?![\w-])` du sidecar — sans consommer le
            // caractère, donc deux occurrences adjacentes sont bien trouvées).
            let inner = Regex::new(&insensitive_pattern(&entries[0].2))
                .expect("alias pattern is static-safe");
            for m in inner.find_iter(text) {
                if !word_boundary_ok(text, m.start(), m.end()) {
                    continue;
                }
                for (key, mention, _form) in entries {
                    let Some(alias) = self.build_alias_span(
                        m.start(),
                        m.end(),
                        mention,
                        key,
                        &canonical_scores,
                        text,
                    ) else {
                        continue;
                    };
                    working = merge_alias(working, &existing, alias);
                }
            }
        }
        working.sort_by_key(|s| (s.start, s.end));
        working
    }

    /// forme normalisée → [(clé canonique, mention, forme originale)].
    fn build_index(
        &self,
        session: &SessionContext,
    ) -> BTreeMap<String, Vec<(String, CanonicalMention, String)>> {
        let mut index: BTreeMap<String, Vec<(String, CanonicalMention, String)>> = BTreeMap::new();
        for m in &session.mentions {
            let mut forms = self.derive(m);
            forms.sort_unstable();
            for form in forms {
                index.entry(normalize_text(&form)).or_default().push((
                    m.key.clone(),
                    m.clone(),
                    form,
                ));
            }
        }
        index
    }

    /// Score de la mention canonique quand elle est spanée dans CE texte
    /// (miroir de `_canonical_scores`).
    fn build_alias_span(
        &self,
        start: usize,
        end: usize,
        mention: &CanonicalMention,
        key: &str,
        canonical_scores: &HashMap<String, f64>,
        text: &str,
    ) -> Option<Span> {
        let base = canonical_scores
            .get(key)
            .copied()
            .unwrap_or(self.config.default_canonical_score);
        // boost borné du seen_count : jamais au-delà de max_score ni du
        // canonique (miroir exact du sidecar).
        let seen = f64::from(mention.seen_count.max(1));
        let boost = (1.0 + 0.02 * (seen - 1.0)).min(1.10);
        let score = (base * self.config.score_cap * boost)
            .min(self.config.max_score)
            .min(base);
        let value = text.get(start..end)?.to_string();
        Some(Span {
            entity_type: mention.kind.clone(),
            start,
            end,
            score: round4(score),
            value,
        })
    }
}

/// Frontière de mot au sens du sidecar (`(?<![\w-])…(?![\w-])`) : la
/// correspondance `[start, end)` ne doit être précédée/suivie d'aucun
/// caractère mot (`\p{L}\p{N}_`) ni d'un trait d'union.
fn word_boundary_ok(text: &str, start: usize, end: usize) -> bool {
    if start > 0 {
        let prev = text[..start].chars().next_back().expect("start > 0");
        if is_word_char_or_hyphen(prev) {
            return false;
        }
    }
    if end < text.len() {
        let next = text[end..].chars().next().expect("end < len");
        if is_word_char_or_hyphen(next) {
            return false;
        }
    }
    true
}

/// Caractère mot (`\p{L}\p{N}_`) ou trait d'union — bloque la frontière.
fn is_word_char_or_hyphen(c: char) -> bool {
    c == '-' || c == '_' || c.is_alphanumeric()
}

/// Score de la mention canonique quand elle est spanée dans CE texte.
fn canonical_scores(text: &str, spans: &[Span], session: &SessionContext) -> HashMap<String, f64> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    for s in spans {
        // PERSON/LOC uniquement (gazetteers nom_sn/ville_sn inclus) — les
        // autres types (MAIL/TEL/CNI/…) ne sont jamais des mentions d'alias.
        let is_person_loc = match &s.entity_type {
            DetectorKind::Person | DetectorKind::Location => true,
            DetectorKind::Gazetteer(n) => n == GAZETTEER_NOM_SN || n == GAZETTEER_VILLE_SN,
            _ => false,
        };
        if !is_person_loc {
            continue;
        }
        let Some(fragment) = text.get(s.start..s.end) else {
            continue;
        };
        let fragment = normalize_text(fragment);
        for m in &session.mentions {
            if fragment == normalize_text(&m.key) {
                let entry = scores.entry(m.key.clone()).or_insert(0.0);
                *entry = entry.max(s.score);
            }
        }
    }
    scores
}

/// Déduplique un alias contre les spans existants (miroir de `_merge_alias`).
fn merge_alias(working: Vec<Span>, existing: &[Span], alias: Span) -> Vec<Span> {
    // Couvert par un span existant → ignoré (l'existant fait foi).
    if existing
        .iter()
        .any(|s| s.start <= alias.start && alias.end <= s.end)
    {
        return working;
    }
    let overlapping: Vec<usize> = working
        .iter()
        .enumerate()
        .filter(|(_, s)| alias.start < s.end && s.start < alias.end)
        .map(|(i, _)| i)
        .collect();
    if overlapping.is_empty() {
        let mut w = working;
        w.push(alias);
        return w;
    }
    // Englobe strictement tous les chevauchements → remplacement.
    if overlapping
        .iter()
        .all(|&i| alias.start <= working[i].start && working[i].end <= alias.end)
    {
        let mut kept: Vec<Span> = working
            .iter()
            .enumerate()
            .filter(|(i, _)| !overlapping.contains(i))
            .map(|(_, s)| s.clone())
            .collect();
        kept.push(alias);
        return kept;
    }
    working
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(key: &str, seen: u32) -> CanonicalMention {
        CanonicalMention {
            key: key.to_string(),
            kind: DetectorKind::Person,
            seen_count: seen,
        }
    }

    fn loc(key: &str) -> CanonicalMention {
        CanonicalMention {
            key: key.to_string(),
            kind: DetectorKind::Location,
            seen_count: 1,
        }
    }

    fn span(text: &str, kind: DetectorKind, start: usize, end: usize, score: f64) -> Span {
        Span {
            entity_type: kind,
            start,
            end,
            score,
            value: text[start..end].to_string(),
        }
    }

    // ------------------------------------------------------------------
    // Dérivation (R1–R7)
    // ------------------------------------------------------------------

    #[test]
    fn derive_r1_r2_r3() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&person("Marie Dupont", 3));
        assert!(forms.contains(&"Marie".to_string()), "R1 prénom seul");
        assert!(forms.contains(&"Dupont".to_string()), "R3 nom seul");
        assert!(forms.contains(&"Mme Dupont".to_string()), "R2 titre + nom");
        assert!(forms.contains(&"M. Dupont".to_string()), "R2");
        assert!(forms.contains(&"Madame Dupont".to_string()), "R2");
    }

    #[test]
    fn derive_excludes_common_names() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&person("Marie Les", 1));
        assert!(
            !forms.contains(&"Les".to_string()),
            "R3 : « Les » est commun"
        );
        assert!(forms.contains(&"Marie".to_string()), "R1 reste");
    }

    #[test]
    fn derive_pronoun_never_leaks() {
        let exp = AliasExpander::new(AliasConfig::default());
        assert!(exp.derive(&person("il", 1)).is_empty());
        assert!(exp.derive(&person("Elle", 1)).is_empty());
        assert!(exp.derive(&person("on", 1)).is_empty());
    }

    #[test]
    fn derive_title_first_token_not_alias() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&person("M. Dupont", 1));
        assert!(
            !forms.contains(&"M.".to_string()),
            "R1 : un titre n'est pas un prénom"
        );
        assert!(forms.contains(&"Dupont".to_string()));
    }

    #[test]
    fn derive_diminutives_r5() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&person("Mamadou", 1));
        assert!(forms.contains(&"Momo".to_string()), "R5 diminutif");
    }

    #[test]
    fn derive_place_shortcuts_r6() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&loc("Ouagadougou"));
        assert!(forms.contains(&"Ouaga".to_string()), "R6 raccourci de lieu");
    }

    #[test]
    fn derive_initial_forms_off_by_default() {
        let exp = AliasExpander::new(AliasConfig::default());
        let forms = exp.derive(&person("Marie Dupont", 1));
        assert!(!forms.contains(&"Marie D.".to_string()));
        let exp2 = AliasExpander::new(AliasConfig {
            enable_initial_forms: true,
            ..Default::default()
        });
        assert!(exp2
            .derive(&person("Marie Dupont", 1))
            .contains(&"Marie D.".to_string()));
    }

    #[test]
    fn derive_max_forms_guard() {
        let cfg = AliasConfig {
            titles: vec![
                "M.".into(),
                "Mme".into(),
                "Mlle".into(),
                "Dr".into(),
                "Pr".into(),
                "Prof".into(),
                "Col".into(),
                "Cdt".into(),
                "Sergent".into(),
                "Major".into(),
                "Général".into(),
                "Générale".into(),
            ],
            ..Default::default()
        };
        let exp = AliasExpander::new(cfg);
        let forms = exp.derive(&person("Marie Dupont", 1));
        assert!(
            !forms.is_empty() && forms.len() <= 8,
            "garde-fou anti-explosion"
        );
    }

    // ------------------------------------------------------------------
    // Expansion
    // ------------------------------------------------------------------

    #[test]
    fn expand_empty_session_noop() {
        let exp = AliasExpander::new(AliasConfig::default());
        let spans = vec![span("Marie est partie", DetectorKind::Person, 0, 5, 0.9)];
        let out = exp.expand("Marie est partie", &spans, &SessionContext::new());
        assert_eq!(out, spans);
    }

    #[test]
    fn expand_alias_basic() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 1));
        let text = "Marie Dupont est partie. Marie reviendra. Mme Dupont aussi.";
        let spans = vec![span(text, DetectorKind::Person, 0, 12, 0.9)];
        let out = exp.expand(text, &spans, &session);
        let aliases: Vec<(String, String)> = out
            .iter()
            .filter(|s| s.entity_type == DetectorKind::Person && s.start >= 12)
            .map(|s| (text[s.start..s.end].to_string(), s.value.clone()))
            .collect();
        assert!(
            aliases.iter().any(|(t, _)| t == "Marie"),
            "R1 : {aliases:?}"
        );
        assert!(
            aliases.iter().any(|(t, _)| t == "Mme Dupont"),
            "R2 : {aliases:?}"
        );
        for s in out.iter().filter(|s| s.start >= 12) {
            assert!(s.score <= 0.9 * 0.85 + 1e-9, "score plafonné ×0.85");
        }
    }

    #[test]
    fn expand_score_cap_and_seen_boost() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 10));
        let text = "Marie Dupont est là. Dupont aussi.";
        let spans = vec![span(text, DetectorKind::Person, 0, 12, 1.0)];
        let out = exp.expand(text, &spans, &session);
        let aliases: Vec<&Span> = out.iter().filter(|s| s.start >= 12).collect();
        assert_eq!(aliases.len(), 1);
        assert_eq!(&text[aliases[0].start..aliases[0].end], "Dupont");
        assert!(aliases[0].score <= 0.95 + 1e-9, "plafond absolu");
        assert!(
            aliases[0].score <= 1.0 * 0.85 * 1.10 + 1e-9,
            "×0.85 × boost borné"
        );
        assert!(aliases[0].score > 0.85, "le seen_count booste (> ×0.85)");
    }

    #[test]
    fn expand_accent_case_insensitive_r7() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Awa Diallo", 1));
        let text = "AWA DIALLO est là. awa est partie.";
        let spans = vec![span(text, DetectorKind::Person, 0, 11, 0.95)];
        let out = exp.expand(text, &spans, &session);
        let alias_texts: Vec<&str> = out
            .iter()
            .filter(|s| s.start >= 11)
            .map(|s| &text[s.start..s.end])
            .collect();
        assert!(
            alias_texts.contains(&"awa"),
            "R7 casse/diacritiques: {alias_texts:?}"
        );
    }

    #[test]
    fn expand_pronoun_never_matched() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 1));
        let out = exp.expand("il est parti. elle aussi.", &[], &session);
        assert!(out.is_empty(), "pronoms jamais matchés: {out:?}");
    }

    #[test]
    fn expand_alias_without_canonical_span() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 1));
        let text = "Marie est partie.";
        let out = exp.expand(text, &[], &session);
        let aliases: Vec<&Span> = out.iter().collect();
        assert_eq!(aliases.len(), 1);
        assert_eq!(&text[aliases[0].start..aliases[0].end], "Marie");
        assert!(
            (aliases[0].score - 0.80 * 0.85).abs() < 1e-6,
            "score canonique par défaut"
        );
    }

    #[test]
    fn expand_dedupe_against_core_spans() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 1));
        let text = "Marie Dupont est là. Marie.";
        // Le « Marie » final (après la mention canonique) est couvert par un
        // span core : l'alias dérivé à la même position doit être ignoré
        // (dans le moteur, les spans core et sidecar sont fusionnés AVANT
        // l'expansion — `existing` contient tous les spans non-alias).
        let second = text.find("Marie").unwrap() + "Marie Dupont est là. ".len();
        let all = vec![
            span(text, DetectorKind::Person, 0, 12, 0.9),
            span(text, DetectorKind::Person, second, second + 5, 1.0),
        ];
        let out = exp.expand(text, &all, &session);
        // Aucun alias ajouté : les deux « Marie » sont couverts par des spans
        // existants (le canonique 0..12 et le core 22..27) — la sortie est
        // exactement les 2 spans d'entrée.
        assert_eq!(
            out.len(),
            2,
            "couvert par le core → aucun alias ajouté: {out:?}"
        );
    }

    #[test]
    fn expand_deterministic() {
        let exp = AliasExpander::new(AliasConfig::default());
        let mut session = SessionContext::new();
        session.mentions.push(person("Marie Dupont", 2));
        session.mentions.push(person("Mamadou Diallo", 1));
        let text = "Marie et Mamadou sont là. Mme Dupont suit. Momo aussi.";
        let spans = vec![span(text, DetectorKind::Person, 0, 12, 0.9)];
        let first = exp.expand(text, &spans, &session);
        let second = exp.expand(text, &spans, &session);
        assert_eq!(first, second, "mêmes entrées → mêmes sorties");
    }

    #[test]
    fn normalize_and_insensitive_pattern() {
        assert_eq!(normalize_text("OUAGADOUGOU"), "ouagadougou");
        assert_eq!(normalize_text("Oùagàdougou"), "ouagadougou");
        let pat = insensitive_pattern("Aïcha");
        assert!(Regex::new(&pat).unwrap().is_match("Aicha"));
        assert!(Regex::new(&pat).unwrap().is_match("AÏCHA"));
    }

    #[test]
    fn session_upsert_increments_seen() {
        let mut s = SessionContext::new();
        s.upsert("Mamadou".to_string(), DetectorKind::Person);
        s.upsert("MAMADOU".to_string(), DetectorKind::Person);
        s.upsert("Dakar".to_string(), DetectorKind::Location);
        assert_eq!(s.mentions.len(), 2);
        assert_eq!(
            s.mentions[0].seen_count, 2,
            "même clé normalisée → seen_count++"
        );
        assert_eq!(s.mentions[1].seen_count, 1);
    }
}
