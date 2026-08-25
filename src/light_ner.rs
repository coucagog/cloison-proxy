//! NER léger embarqué (chantier ④, N0 v1.2 — journal/ARBITRAGE-04).
//!
//! Producteur de spans PERSON/LOC **local au daemon N0** (jamais un sidecar
//! Python — charte §4 : « le daemon reste moteur Rust seul ») : le proxy
//! tokenise (crate `tokenizers`, tokenizer.json BERT/distilbert), infère via
//! ONNX Runtime (`ort` 2.x, lib chargée dynamiquement) et aligne les spans
//! (portage de `_align_spans` du sidecar `cloison-detect`).
//!
//! Le core reste la **source de vérité** de la tokenisation : les spans
//! produits ici sont passés à `Engine::tokenize_session(extra)` (exactement
//! le rôle du sidecar distant B.1), avec la **fusion englobante** N0
//! (`SessionOptions.enable_enclosing_ner_fusion`) : un span NER complet prime
//! sur les fragments gazetteer qu'il englobe.
//!
//! Dégradation gracieuse OBLIGATOIRE (ARBITRAGE-04 §4.3) : modèle absent,
//! lib onnxruntime absente, tokenizer invalide, prédiction en échec → le
//! daemon reste en N0 v1 (gazetteers + alias), `warn`, jamais d'erreur.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use cloison_core::detection::{DetectorKind, Span};

pub use crate::config::LightNerConfig;

/// NER léger embarqué (PERSON/LOC). Stateless après chargement ; `Send+Sync`
/// (partagé par `Arc` dans `AppState`). La session ONNX est gardée dans un
/// `Mutex` (la `Session` ort n'est pas `Clone` et `run` prend `&mut self` —
/// l'inférence des requêtes concurrentes est sérialisée, comme le verrou de
/// chargement du sidecar Python).
pub struct LightNer {
    config: LightNerConfig,
    session: Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
    /// id2label (label_map.json à côté du modèle — convention DEPLOY-8).
    labels: Vec<(i64, String)>,
}

// ort::session::Session + tokenizers::Tokenizer sont Send+Sync.
static _ASSERT_SEND_SYNC: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LightNer>();
};

impl LightNer {
    /// Charge le NER léger. Retourne `None` (jamais une erreur) si un
    /// composant manque ou échoue — dégradation gracieuse (N0 v1 inchangé).
    pub fn try_new(config: LightNerConfig) -> Option<Self> {
        if !config.model_path.exists() || !config.tokenizer_path.exists() {
            tracing::warn!(
                model = %config.model_path.display(),
                tokenizer = %config.tokenizer_path.display(),
                "NER léger embarqué indisponible (modèle ou tokenizer absents) — N0 v1 inchangé"
            );
            return None;
        }
        // 1) init onnxruntime dynamique (une seule fois par processus).
        if let Err(e) = Self::ensure_ort_init(&config) {
            tracing::warn!(
                detail = ?e,
                "NER léger embarqué indisponible (lib onnxruntime) — N0 v1 inchangé"
            );
            return None;
        }
        // 2) tokenizer.
        let tokenizer = match tokenizers::Tokenizer::from_file(&config.tokenizer_path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(detail = %e, "NER léger embarqué indisponible (tokenizer)");
                return None;
            }
        };
        // 3) session ONNX.
        let session = match ort::session::builder::SessionBuilder::new()
            .and_then(|mut b| b.commit_from_file(&config.model_path))
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    detail = ?e,
                    "NER léger embarqué indisponible (session ONNX) — N0 v1 inchangé"
                );
                return None;
            }
        };
        // 4) label_map (optionnel — sans lui, les IDs sont utilisés tels quels).
        let labels = load_label_map(&config.model_path);
        tracing::info!(
            model = %config.model_path.display(),
            labels = labels.len(),
            threshold = config.threshold,
            "NER léger embarqué actif (chantier ④) — PERSON/LOC in-core"
        );
        Some(Self {
            config,
            session: Mutex::new(session),
            tokenizer,
            labels,
        })
    }

    /// Initialise la lib onnxruntime une fois par processus. `init_from`
    /// charge la lib dans un `OnceLock` global (elle reste chargée) et
    /// retourne un `EnvironmentBuilder` jetable.
    fn ensure_ort_init(config: &LightNerConfig) -> Result<(), String> {
        static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();
        ORT_INIT
            .get_or_init(|| {
                let lib = config
                    .onnx_lib
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("libonnxruntime.so"));
                match ort::init_from(Path::new(&lib)) {
                    Ok(_builder) => Ok(()),
                    Err(e) => Err(format!("{e:?}")),
                }
            })
            .clone()
    }

    /// Détecte PERSON/LOC dans `text` (offsets **octets** — contrat interne
    /// du core, `merge_*_spans` : `text.len()`, `is_char_boundary` et
    /// `text[start..end]` sont des octets Rust).
    pub fn detect(&self, text: &str) -> Vec<Span> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let encoding = match self.tokenizer.encode(text, false) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(detail = %e, "NER léger : tokenisation échouée — spans ignorés");
                return Vec::new();
            }
        };
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        if ids.is_empty() {
            return Vec::new();
        }
        // Le tokenizer `tokenizers` (HF) renvoie des IDs u32 ; le graphe ONNX
        // attend int64 (input_ids/attention_mask) — conversion explicite.
        let ids_i64: Vec<i64> = ids.iter().map(|&i| i as i64).collect();
        let mask_i64: Vec<i64> = mask.iter().map(|&i| i as i64).collect();
        let input_ids = match ort::value::Tensor::from_array((vec![1usize, ids.len()], ids_i64)) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(detail = ?e, "NER léger : tensor input_ids — spans ignorés");
                return Vec::new();
            }
        };
        let attention_mask = match ort::value::Tensor::from_array((
            vec![1usize, mask.len()],
            mask_i64,
        )) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(detail = ?e, "NER léger : tensor attention_mask — spans ignorés");
                return Vec::new();
            }
        };

        // Entrées selon le graphe (BERT demande token_type_ids, distilbert non).
        let mut session = match self.session.lock() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(detail = %e, "NER léger : verrou session — spans ignorés");
                return Vec::new();
            }
        };
        let wanted: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let run = if wanted.iter().any(|n| n == "token_type_ids") {
            let seg = vec![0i64; ids.len()];
            match ort::value::Tensor::from_array((vec![1usize, ids.len()], seg)) {
                Ok(tt) => session.run(ort::inputs![input_ids, attention_mask, tt]),
                Err(e) => {
                    tracing::warn!(detail = ?e, "NER léger : tensor token_type_ids — spans ignorés");
                    return Vec::new();
                }
            }
        } else {
            session.run(ort::inputs![input_ids, attention_mask])
        };

        let outputs = match run {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(detail = ?e, "NER léger : inférence échouée — spans ignorés");
                return Vec::new();
            }
        };
        // Sortie : le tenseur `logits` (1, seq, labels). `get("logits")` cible
        // le nom canonique ; fallback = premier output (graphes sans nom).
        let first_output = outputs.values().next();
        let logits_value = outputs.get("logits").or(first_output.as_deref());
        let Some(logits_value) = logits_value else {
            return Vec::new();
        };
        // Logits (1, seq, labels) en f32.
        let (shape, logits): (&ort::value::Shape, &[f32]) =
            match logits_value.try_extract_tensor::<f32>() {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            };
        if shape.as_ref().len() != 3 {
            return Vec::new();
        }
        let seq_len = shape.as_ref()[1] as usize;
        let num_labels = shape.as_ref()[2] as usize;
        if seq_len == 0 || num_labels == 0 {
            return Vec::new();
        }

        // argmax + softmax (équivalent `_detect_onnx` du sidecar).
        let mut pred_ids = Vec::with_capacity(seq_len);
        let mut probs = vec![0f32; seq_len];
        for (s_idx, p_out) in probs.iter_mut().enumerate() {
            let base = s_idx * num_labels;
            let mut best = 0usize;
            let mut best_v = logits[base];
            for l in 1..num_labels {
                let v = logits[base + l];
                if v > best_v {
                    best_v = v;
                    best = l;
                }
            }
            pred_ids.push(best as i64);
            // softmax stable sur la ligne.
            let row: Vec<f32> = (0..num_labels).map(|l| logits[base + l]).collect();
            let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let exps: Vec<f32> = row.iter().map(|v| (v - max).exp()).collect();
            let sum: f32 = exps.iter().sum();
            *p_out = exps[best] / sum;
        }

        // Alignement offsets → spans (portage `_align_spans`).
        // NB : le crate `tokenizers` (HF) rend les offsets des tokens en
        // **octets** relatifs au texte d'origine — exactement le contrat du
        // core (`text.len()`, `is_char_boundary`, `text[start..end]` sont
        // des octets Rust). Aucune conversion nécessaire.
        let offsets = encoding.get_offsets();
        self.align_spans(&pred_ids, &probs, offsets)
    }

    /// Aligne tokens → offsets caractères ; regroupe les tokens contigus de
    /// même type (gère BIO). Miroir de `AfricanModelDetector._align_spans`.
    fn align_spans(
        &self,
        pred_ids: &[i64],
        probs: &[f32],
        offsets: &[(usize, usize)],
    ) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        let mut cur: Option<(DetectorKind, usize, usize, Vec<f32>)> = None;

        let label_of = |id: i64| -> Option<DetectorKind> {
            let raw = self
                .labels
                .iter()
                .find(|(k, _)| *k == id)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| id.to_string());
            let core = if raw.starts_with("B-")
                || raw.starts_with("I-")
                || raw.starts_with("E-")
                || raw.starts_with("S-")
            {
                raw.split('-').nth(1).unwrap_or(&raw).to_string()
            } else {
                raw.clone()
            };
            match core.to_uppercase().as_str() {
                "PER" | "PERSON" => Some(DetectorKind::Person),
                "LOC" | "LOCATION" | "GPE" => Some(DetectorKind::Location),
                _ => None,
            }
        };

        let flush = |cur: &mut Option<(DetectorKind, usize, usize, Vec<f32>)>,
                     spans: &mut Vec<Span>| {
            if let Some((kind, start, end, probs_list)) = cur.take() {
                if !probs_list.is_empty() {
                    let score = probs_list.iter().sum::<f32>() / probs_list.len() as f32;
                    if score >= self.config.threshold as f32 {
                        spans.push(Span {
                            entity_type: kind,
                            start,
                            end,
                            score: f64::from(score),
                            value: String::new(), // re-tranchée par le core
                        });
                    }
                }
            }
        };

        for (i, (tok_start, tok_end)) in offsets.iter().enumerate() {
            if *tok_start >= *tok_end {
                flush(&mut cur, &mut spans);
                continue;
            }
            let kind = label_of(pred_ids[i]);
            let extends = match (&cur, &kind) {
                (Some((k, _, end, _)), Some(nk)) => *k == *nk && *end == *tok_start,
                _ => false,
            };
            if extends {
                if let (Some((_, _, end, probs_list)), Some(_)) = (&mut cur, &kind) {
                    *end = *tok_end;
                    probs_list.push(probs[i]);
                }
            } else {
                flush(&mut cur, &mut spans);
                if let Some(nk) = kind {
                    cur = Some((nk, *tok_start, *tok_end, vec![probs[i]]));
                }
            }
        }
        flush(&mut cur, &mut spans);
        spans
    }
}

/// Charge `label_map.json` (id2label) à côté du modèle ONNX — convention
/// DEPLOY-8. Vide si absent (les IDs numériques servent alors de labels).
fn load_label_map(model_path: &Path) -> Vec<(i64, String)> {
    let dir = model_path.parent().unwrap_or(Path::new("."));
    let labels_path = dir.join("label_map.json");
    let Ok(text) = std::fs::read_to_string(&labels_path) else {
        return Vec::new();
    };
    let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(&text) else {
        return Vec::new();
    };
    let mut out: Vec<(i64, String)> = map
        .into_iter()
        .filter_map(|(k, v)| k.parse::<i64>().ok().map(|n| (n, v)))
        .collect();
    out.sort_by_key(|(k, _)| *k);
    out
}
