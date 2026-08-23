//! Reçu signé d'une requête auditée.
//!
//! # Règle absolue : jamais de texte
//!
//! Un `Receipt` ne contient **que des compteurs entiers** (`Counters`) et des
//! identifiants non sensibles (`tenant_id`, références **hachées**, versions,
//! hash de politique). Aucun span, aucune valeur claire, aucun texte n'y
//! circule : même un bug dans l'agrégateur ne peut pas faire fuiter du texte
//! vers le corpus (séparation corpus garantie par construction).
//!
//! # Format de signature — message canonique
//!
//! Le message signé est le **JSON canonique** des champs du reçu hors
//! signature : sérialisation `serde_json` **sans espace blanc**, **ordre des
//! clés stable** (les champs de structure sont sérialisés dans l'ordre de
//! déclaration, et `masked_by_type` est un `BTreeMap` → clés triées). Deux
//! machines construisant le même reçu produisent donc exactement les mêmes
//! octets signés (testé par `signing_bytes_are_deterministic`).
//!
//! `sig_agent` = Ed25519 pur (RFC 8032, déterministe) de ces octets, signée
//! par la clé de l'agent au bord. La vérification utilise `verify_strict`
//! (rejet de la malléabilité de signature).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AuditError, AuditResult};

/// Version du schéma de reçu. Toute évolution cassante incrémente cette valeur.
pub const AUDIT_SCHEMA_VERSION: u8 = 1;

/// Compteurs d'une requête auditée. **Entiers uniquement — aucun texte.**
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    /// Occurrences détectées par type de PII (masqué potentiel : ce que le
    /// moteur de masquage *aurait* remplacé). Clés = `Display` de
    /// `cloison_core::DetectorKind` ("Email", "PhoneSn", "CniSn"…).
    /// `BTreeMap` → sérialisation canonique à clés triées.
    pub masked_by_type: BTreeMap<String, u64>,
    /// Restaurations incomplètes : sentinelles trouvées dans une réponse mais
    /// non résolubles (aucune sentinelle n'est légitime en mode audit).
    pub incomplete_restorations: u64,
    /// Sorties bloquées : sentinelles invalides/malformées qui, en mode
    /// masquage, auraient fait passer le champ en marqueur neutre (fail-loud).
    pub blocked_outputs: u64,
    /// Drapeaux quasi-identifiants : occurrences de types à faible cardinalité
    /// (IP, date, carte bancaire) qui auraient été **généralisées** plutôt que
    /// tokenisées — elles signalent un risque de ré-identification résiduel.
    pub quasi_id_flags: u64,
}

impl Counters {
    /// Cumule un autre jeu de compteurs dans `self` (somme champ à champ).
    pub fn add(&mut self, other: &Counters) {
        for (kind, count) in &other.masked_by_type {
            *self.masked_by_type.entry(kind.clone()).or_insert(0) += count;
        }
        self.incomplete_restorations += other.incomplete_restorations;
        self.blocked_outputs += other.blocked_outputs;
        self.quasi_id_flags += other.quasi_id_flags;
    }
}

/// Champs d'un reçu avant signature (le « message » passé à `build`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptMessage {
    /// Identifiant locataire (en clair — non sensible).
    pub tenant_id: String,
    /// Référence de session **hachée** (SHA-256) : ne révèle ni la session
    /// ni sa clé.
    pub session_ref_hashed: String,
    /// Horodatage Unix (secondes, UTC).
    pub ts_unix: u64,
    /// Version du moteur (ex. `env!("CARGO_PKG_VERSION")`).
    pub engine_version: String,
    /// `hex(SHA-256(json canonique de la Policy))` — traçabilité de la règle
    /// appliquée au comptage.
    pub policy_hash: String,
    /// Compteurs (jamais de texte).
    pub counters: Counters,
}

/// Reçu signé d'une requête en mode audit.
///
/// La signature couvre `signing_bytes()` (JSON canonique des champs hors
/// `sig_agent`) ; le reçu re-sérialisé en JSON ne fait pas partie du message
/// signé — le vérificateur reconstruit `signing_bytes()` depuis les champs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Identifiant locataire (en clair — non sensible).
    pub tenant_id: String,
    /// Référence de session hachée (SHA-256 hex).
    pub session_ref_hashed: String,
    /// Horodatage Unix (secondes, UTC).
    pub ts_unix: u64,
    /// Version du moteur.
    pub engine_version: String,
    /// `hex(SHA-256(json canonique de la Policy))`.
    pub policy_hash: String,
    /// Compteurs (jamais de texte).
    pub counters: Counters,
    /// Signature Ed25519 sur `signing_bytes()` (64 octets bruts).
    pub sig_agent: Vec<u8>,
}

impl Receipt {
    /// Construit un reçu **non signé** depuis un message.
    ///
    /// `sig_agent` est vide ; appeler [`Receipt::sign`] ensuite.
    pub fn build(message: ReceiptMessage) -> Self {
        Self {
            tenant_id: message.tenant_id,
            session_ref_hashed: message.session_ref_hashed,
            ts_unix: message.ts_unix,
            engine_version: message.engine_version,
            policy_hash: message.policy_hash,
            counters: message.counters,
            sig_agent: Vec::new(),
        }
    }

    /// Construit un reçu **signé** en une étape.
    pub fn build_signed(message: ReceiptMessage, signing_key: &SigningKey) -> Self {
        Self::build(message).sign(signing_key)
    }

    /// Le message exact qui est signé.
    ///
    /// JSON canonique des champs du reçu hors `sig_agent` : sérialisation
    /// `serde_json` compacte (aucun espace blanc), ordre des champs fixe
    /// (déclaration) et clés triées (`BTreeMap`). Déterministe — voir la
    /// doc du module.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let message = ReceiptMessage {
            tenant_id: self.tenant_id.clone(),
            session_ref_hashed: self.session_ref_hashed.clone(),
            ts_unix: self.ts_unix,
            engine_version: self.engine_version.clone(),
            policy_hash: self.policy_hash.clone(),
            counters: self.counters.clone(),
        };
        // La sérialisation d'un struct serde est sans espace et dans l'ordre
        // de déclaration ; `masked_by_type` (BTreeMap) garantit les clés
        // triées → octets identiques entre machines.
        serde_json::to_vec(&message).expect("canonical receipt serialization is infallible")
    }

    /// Signe ce reçu (remplit `sig_agent`) et renvoie un nouveau reçu.
    ///
    /// La signature est déterministe (Ed25519 RFC 8032).
    pub fn sign(&self, signing_key: &SigningKey) -> Receipt {
        let sig: Signature = signing_key.sign(&self.signing_bytes());
        let mut signed = self.clone();
        signed.sig_agent = sig.to_bytes().to_vec();
        signed
    }

    /// Vérifie `sig_agent` contre une clé publique.
    ///
    /// Reconstruit `signing_bytes()` puis `verify_strict` (rejette les
    /// signatures malléables). `false` si `sig_agent` est vide ou invalide.
    pub fn verify(&self, verify_key: &VerifyingKey) -> bool {
        if self.sig_agent.len() != 64 {
            return false;
        }
        let Ok(sig) = Signature::from_slice(&self.sig_agent) else {
            return false;
        };
        verify_key
            .verify_strict(&self.signing_bytes(), &sig)
            .is_ok()
    }

    /// Encode le reçu en `base64url(canonical_json(receipt))` — valeur du
    /// header `X-Cloison-Audit-Receipt` (URL-safe, sans padding).
    pub fn to_base64url_json(&self) -> String {
        let json = serde_json::to_vec(self).expect("receipt serialization is infallible");
        base64url_encode(&json)
    }

    /// Décode un reçu depuis `base64url(canonical_json(receipt))`.
    pub fn from_base64url_json(encoded: &str) -> AuditResult<Self> {
        let bytes = base64url_decode(encoded)?;
        serde_json::from_slice(&bytes).map_err(AuditError::from)
    }
}

/// `hex(SHA-256(tenant_id ‖ ":" ‖ session_ref))` — ne révèle ni la référence
/// de session ni sa clé.
pub fn hash_session_ref(tenant_id: &str, session_ref: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tenant_id.as_bytes());
    hasher.update(b":");
    hasher.update(session_ref.as_bytes());
    hex_bytes(&hasher.finalize())
}

/// `policy_hash = hex(SHA-256(json canonique de la Policy))`.
///
/// La Policy contient des `HashSet`/`HashMap` (ordre d'itération non garanti) :
/// le JSON est donc **normalisé canoniquement** avant hashage :
/// - les objets sont re-triés par clé (sans `preserve_order`, `serde_json::Map`
///   est déjà un `BTreeMap` ; le tri est rendu explicite pour rester robuste) ;
/// - les tableaux **scalaires uniquement** (sérialisation d'un `HashSet`) sont
///   triés — un ensemble n'a pas d'ordre, deux permutations doivent produire
///   le même hash ;
/// - aucun espace blanc.
pub fn policy_hash(policy: &cloison_core::Policy) -> AuditResult<String> {
    let mut value = serde_json::to_value(policy)?;
    canonicalize(&mut value);
    let canonical = serde_json::to_string(&value)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(hex_bytes(&hasher.finalize()))
}

/// Normalisation canonique récursive d'une valeur JSON (voir `policy_hash`).
fn canonicalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let taken = std::mem::take(map);
            let mut entries: Vec<(String, serde_json::Value)> = taken.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, v) in entries.iter_mut() {
                canonicalize(v);
            }
            *map = entries.into_iter().collect();
        }
        serde_json::Value::Array(arr) => {
            // Normalise d'abord les éléments (objets imbriqués, ex.
            // DetectorKind::Gazetteer -> {"Gazetteer":"nom_sn"})…
            for v in arr.iter_mut() {
                canonicalize(v);
            }
            // …puis TRI : un tableau JSON issu d'un HashSet/HashMap n'a PAS
            // d'ordre d'itération déterministe — le hash canonique (I-A4)
            // doit trier aussi les tableaux mixtes (tri par représentation
            // canonique, stable entre machines).
            arr.sort_by_key(|a| a.to_string());
        }
        _ => {}
    }
}

/// Horodatage Unix courant (secondes, UTC) ; 0 si l'horloge système précède
/// l'époque (ne doit pas arriver).
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Encodage base64url sans padding (RFC 4648 §5).
fn base64url_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (chunk.get(1).copied().unwrap_or(0) as u32) << 8
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// Décodage base64url sans padding (tolère un padding `=` résiduel).
fn base64url_decode(encoded: &str) -> AuditResult<Vec<u8>> {
    let chars: Vec<char> = encoded.trim_end_matches('=').chars().collect();
    if chars.len() % 4 == 1 {
        return Err(AuditError::InvalidBase64("length mod 4 == 1".to_string()));
    }
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= chars.len() {
        let n = base64url_value(&chars[i..i + 4])?;
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    match chars.len() - i {
        0 => {}
        2 => {
            let n = base64url_value(&[chars[i], chars[i + 1], 'A', 'A'])?;
            out.push((n >> 16) as u8);
        }
        3 => {
            let n = base64url_value(&[chars[i], chars[i + 1], chars[i + 2], 'A'])?;
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => return Err(AuditError::InvalidBase64("invalid tail".to_string())),
    }
    Ok(out)
}

/// 4 caractères base64url → valeur 24 bits.
fn base64url_value(chars: &[char]) -> AuditResult<u32> {
    let mut n: u32 = 0;
    for &c in chars {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '-' => 62,
            '_' => 63,
            _ => return Err(AuditError::InvalidBase64(format!("invalid char {c:?}"))),
        };
        n = (n << 6) | v;
    }
    Ok(n)
}

/// hex minuscule d'octets.
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_roundtrip() {
        for data in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            &[0u8, 1, 2, 250, 251, 252][..],
        ] {
            let enc = base64url_encode(data);
            assert_eq!(base64url_decode(&enc).unwrap(), data, "roundtrip {data:?}");
        }
    }

    #[test]
    fn base64url_known_vectors() {
        // RFC 4648 §10 vectors (URL-safe alphabet, no padding).
        assert_eq!(base64url_encode(b""), "");
        assert_eq!(base64url_encode(b"f"), "Zg");
        assert_eq!(base64url_encode(b"fo"), "Zm8");
        assert_eq!(base64url_encode(b"foo"), "Zm9v");
        assert_eq!(base64url_encode(b"foob"), "Zm9vYg");
        assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64url_rejects_invalid() {
        assert!(base64url_decode("a").is_err());
        assert!(base64url_decode("ab!c").is_err());
        assert!(base64url_decode("abcd.").is_err());
    }

    #[test]
    fn counters_add_sums_fields() {
        let mut a = Counters::default();
        a.masked_by_type.insert("Email".to_string(), 2);
        a.incomplete_restorations = 1;
        a.blocked_outputs = 3;
        a.quasi_id_flags = 4;
        let mut b = Counters::default();
        b.masked_by_type.insert("Email".to_string(), 3);
        b.masked_by_type.insert("PhoneSn".to_string(), 1);
        b.blocked_outputs = 2;
        a.add(&b);
        assert_eq!(a.masked_by_type.get("Email"), Some(&5));
        assert_eq!(a.masked_by_type.get("PhoneSn"), Some(&1));
        assert_eq!(a.incomplete_restorations, 1);
        assert_eq!(a.blocked_outputs, 5);
        assert_eq!(a.quasi_id_flags, 4);
    }

    #[test]
    fn hash_session_ref_is_stable_and_oblivious() {
        let h1 = hash_session_ref("tenant-42", "sess-abc");
        let h2 = hash_session_ref("tenant-42", "sess-abc");
        let h3 = hash_session_ref("tenant-42", "sess-xyz");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64, "sha256 hex length");
        assert!(!h1.contains("sess-abc"), "session ref must not leak");
    }
}
