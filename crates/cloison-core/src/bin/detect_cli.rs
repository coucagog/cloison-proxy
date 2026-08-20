//! CLI minimal de détection pour le différentiel Presidio et le benchmark GO/NO-GO.
//! Lit un texte sur stdin, émet les spans JSON sur stdout.
//! Les offsets sont exprimés en POINTS DE CODE (compatibles Python/grille),
//! convertis depuis les offsets bytes de `regex` (Rust).
//! Usage: echo "texte" | cloison-detect-cli

use cloison_core::detection::Detector;
use cloison_core::policy::DetectorPolicy;
use std::io::Read;

fn main() {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .expect("read stdin");

    let detector = Detector::new().expect("detector");
    let policy = DetectorPolicy::default();
    let spans = detector.detect_with_policy(&text, &policy);

    // Convertit un offset byte (Rust regex) en offset points de code (contrat).
    let byte_to_char = |byte_off: usize| -> usize {
        text[..byte_off].chars().count()
    };

    let out: Vec<serde_json::Value> = spans
        .iter()
        .map(|s| {
            serde_json::json!({
                "start": byte_to_char(s.start),
                "end": byte_to_char(s.end),
                "type": s.entity_type.to_string(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string(&out).expect("json"));
}
