//! Substitution par **faux réaliste** (`SubstitutionMode::RealisticFake`).
//!
//! Le faux est :
//! - **déterministe dans la session** : même valeur → même faux (la graine est
//!   le corps du jeton, dérivé HMAC(clé_locataire ‖ sel_session, valeur) —
//!   cohérence de coréférence conservée) ;
//! - **différent entre sessions** (rotation du sel) ;
//! - **jamais la valeur réelle** (les listes sont génériques, la dérivation
//!   ne contient aucun dictionnaire inverse).
//!
//! **IRRÉVERSIBLE** : rien n'est enregistré pour la restauration — le modèle
//! voit du texte normal (insensible au nettoyage des sentinelles ⟦…⟧), mais
//! la valeur d'origine ne revient JAMAIS au client. Usage réservé aux
//! politiques qui l'assument (charte §6.2, §11 — un faux réaliste rend
//! l'échec invisible : ne PAS mélanger avec des sentinelles sans vérifier la
//! restauration complète avant émission).
//!
//! Types couverts : PERSON / Gazetteer(nom_sn) / PhoneSn / Email.
//! Autres types → `None` (l'engine retombe sur la sentinelle — sûr).

use crate::detection::DetectorKind;
use crate::token::{SessionKeys, Token};

const PRENOMS: &[&str] = &[
    "Aminata", "Fatou", "Awa", "Mariama", "Khady", "Astou", "Ndeye", "Bineta",
    "Coumba", "Rokhaya", "Maimouna", "Sokhna", "Adja", "Dieynaba", "Yacine", "Seynabou",
];
const NOMS: &[&str] = &[
    "Diop", "Ndiaye", "Fall", "Sarr", "Ba", "Sy", "Diallo", "Gueye",
    "Cisse", "Mbaye", "Sow", "Faye", "Kane", "Lo", "Niang", "Diagne",
];
const PREFIXES_TEL: &[&str] = &["70", "75", "76", "77", "78"];

/// Graine déterministe : le corps du jeton (8 octets, dérivé clé+sel+MAC).
fn seed_bytes(plain_value: &str, kind: &DetectorKind, keys: &SessionKeys) -> crate::CloisonResult<Vec<u8>> {
    let token = Token::emit(plain_value, kind, keys)?;
    Ok(token.body.as_bytes().to_vec())
}

fn pick<'a>(list: &'a [&'a str], seed: &[u8], slot: usize) -> &'a str {
    list[seed[slot % seed.len()] as usize % list.len()]
}

fn fake_name(plain_value: &str, seed: &[u8]) -> String {
    let words: Vec<&str> = plain_value.split_whitespace().collect();
    if words.len() >= 2 {
        format!("{} {}", pick(PRENOMS, seed, 0), pick(NOMS, seed, 1))
    } else {
        pick(PRENOMS, seed, 0).to_string()
    }
}

fn fake_phone(seed: &[u8]) -> String {
    let mut digits = String::new();
    for i in 1..=7 {
        digits.push((b'0' + seed[i % seed.len()] % 10) as char);
    }
    // +221 7X XXX XX XX (9 chiffres après l'indicatif)
    format!(
        "+221 {} {} {} {}",
        pick(PREFIXES_TEL, seed, 0),
        &digits[0..3],
        &digits[3..5],
        &digits[5..7]
    )
}

fn fake_email(seed: &[u8]) -> String {
    let mut local = String::new();
    for i in 0..8 {
        local.push((b'a' + seed[i % seed.len()] % 26) as char);
    }
    format!("{local}@example.sn")
}

/// Retourne le faux réaliste pour les types couverts, `None` sinon.
pub fn fake_value(
    plain_value: &str,
    kind: &DetectorKind,
    keys: &SessionKeys,
) -> crate::CloisonResult<Option<String>> {
    let seed = seed_bytes(plain_value, kind, keys)?;
    Ok(match kind {
        DetectorKind::Person => Some(fake_name(plain_value, &seed)),
        DetectorKind::Gazetteer(n) if n == crate::detection::GAZETTEER_NOM_SN => {
            Some(fake_name(plain_value, &seed))
        }
        DetectorKind::PhoneSn => Some(fake_phone(&seed)),
        DetectorKind::Email => Some(fake_email(&seed)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::SessionKeys;

    fn keys(salt_byte: u8) -> SessionKeys {
        SessionKeys::derive([0x42u8; 32], [salt_byte; 16]).unwrap()
    }

    #[test]
    fn fake_is_deterministic_within_session() {
        let k = keys(1);
        let a = fake_value("Aminata Diop", &DetectorKind::Person, &k).unwrap();
        let b = fake_value("Aminata Diop", &DetectorKind::Person, &k).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn fake_rotates_between_sessions() {
        let a = fake_value("Aminata Diop", &DetectorKind::Person, &keys(1)).unwrap();
        let b = fake_value("Aminata Diop", &DetectorKind::Person, &keys(2)).unwrap();
        assert_ne!(a, b, "rotation par sel de session");
    }

    #[test]
    fn fake_never_contains_original() {
        let k = keys(1);
        for (val, kind) in [
            ("Aminata Diop", DetectorKind::Person),
            ("+221 77 123 45 67", DetectorKind::PhoneSn),
            ("aminata.diop@example.sn", DetectorKind::Email),
        ] {
            let fake = fake_value(val, &kind, &k).unwrap().unwrap();
            let norm = |s: &str| s.to_lowercase();
            assert!(
                !norm(&fake).contains(&norm(val)),
                "le faux ne doit pas contenir l'original ({fake})"
            );
        }
    }

    #[test]
    fn fake_phone_and_email_formats() {
        let k = keys(1);
        let tel = fake_value("+221 77 123 45 67", &DetectorKind::PhoneSn, &k).unwrap().unwrap();
        assert!(tel.starts_with("+221 7"), "format tel : {tel}");
        assert_eq!(tel.chars().filter(|c| c.is_ascii_digit()).count(), 12, "tel : {tel}");
        let mail = fake_value("x@y.sn", &DetectorKind::Email, &k).unwrap().unwrap();
        assert!(mail.ends_with("@example.sn"), "email : {mail}");
    }

    #[test]
    fn fake_name_mirrors_word_count() {
        let k = keys(1);
        let one = fake_value("Aminata", &DetectorKind::Person, &k).unwrap().unwrap();
        let two = fake_value("Aminata Diop", &DetectorKind::Person, &k).unwrap().unwrap();
        assert_eq!(one.split_whitespace().count(), 1);
        assert_eq!(two.split_whitespace().count(), 2);
    }

    #[test]
    fn unsupported_kind_returns_none() {
        let k = keys(1);
        assert!(fake_value("1234567890123", &DetectorKind::CniSn, &k).unwrap().is_none());
        assert!(fake_value("Dakar", &DetectorKind::Gazetteer("ville_sn".into()), &k).unwrap().is_none());
    }
}
