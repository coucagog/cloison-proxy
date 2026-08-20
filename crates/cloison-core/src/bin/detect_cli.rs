//! CLI minimal de détection pour le différentiel Presidio.
//! Lit un texte sur stdin, émet les spans JSON sur stdout.
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

    let out: Vec<serde_json::Value> = spans
        .iter()
        .map(|s| {
            serde_json::json!({
                "start": s.start,
                "end": s.end,
                "type": s.entity_type.to_string(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string(&out).expect("json"));
}
