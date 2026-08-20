//! Contresignature des reçus d'audit (STACK-4).
//!
//! L'agent au bord signe son reçu sur **`Receipt::signing_bytes()`** — le JSON canonique
//! complet du reçu (voir `cloison-audit/src/receipt.rs`), **jamais** un digest de 32
//! octets. Le contrôle :
//!
//! 1. **vérifie** `sig_agent` sur ce même message (`Receipt::verify`, `verify_strict`) ;
//! 2. **ajoute** sa propre signature `sig_control` (Ed25519) sur le même message.
//!
//! Deux clés distinctes, deux responsabilités, vérification indépendante. À l'ingest,
//! la signature de l'entrée du journal (`LedgerEntry.sig`) lie ensuite `entry_hash`
//! (seq/prev/payload/ts) — c'est la contresignature au niveau du journal.
//!
//! Zéro PII : seuls des hash (32 octets) et des signatures (64 octets) transitent ici —
//! jamais de texte, jamais de jeton, jamais de valeur.

use crate::error::{ControlError, ControlResult};
use cloison_audit::Receipt;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Longueur d'une signature Ed25519.
pub const SIGNATURE_LEN: usize = 64;

/// Reçu contresigné : les deux signatures couvrent le **même message** —
/// `receipt.signing_bytes()` (JSON canonique complet du reçu STACK-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contresignature {
    /// `SHA-256(receipt.signing_bytes())` — engagement du message canonique signé.
    pub message_hash: [u8; 32],
    /// Signature de l'agent (celle d'origine, vérifiée avant contresignature).
    pub sig_agent: Vec<u8>,
    /// Signature du contrôle (`control_signing_key`), ajoutée après vérification.
    pub sig_control: Vec<u8>,
}

/// Contresigne un reçu d'audit :
/// 1. vérifie `sig_agent` (Ed25519, `verify_strict`) sur **`receipt.signing_bytes()`** —
///    le MÊME message que l'agent a signé (STACK-4, `receipt.rs::sign`) ;
/// 2. si valide, produit `sig_control = Ed25519(control_key, signing_bytes())`.
///
/// Erreur [`ControlError::InvalidAgentSignature`] si la signature de l'agent est
/// invalide — rien n'est contresigné dans ce cas.
pub fn contresigner_reçu(
    receipt: &Receipt,
    agent_verify_key: &VerifyingKey,
    control_signing_key: &SigningKey,
) -> ControlResult<Contresignature> {
    // Receipt::verify reconstruit signing_bytes() puis verify_strict.
    if !receipt.verify(agent_verify_key) {
        return Err(ControlError::InvalidAgentSignature);
    }
    let message = receipt.signing_bytes();
    let sig_control = control_signing_key.sign(&message);
    Ok(Contresignature {
        message_hash: sha256(&message),
        sig_agent: receipt.sig_agent.clone(),
        sig_control: sig_control.to_bytes().to_vec(),
    })
}

/// Vérifie une contresignature contre le reçu (message = `receipt.signing_bytes()`) :
/// les deux signatures (`sig_agent` et `sig_control`) doivent être valides sur ce message.
///
/// Utile au proxy, aux vérificateurs hors-ligne et aux tests.
pub fn verifier_contresignature(
    contresignature: &Contresignature,
    receipt: &Receipt,
    agent_verify_key: &VerifyingKey,
    control_verify_key: &VerifyingKey,
) -> bool {
    if contresignature.message_hash != sha256(&receipt.signing_bytes()) {
        return false;
    }
    if contresignature.sig_agent != receipt.sig_agent {
        return false;
    }
    if !receipt.verify(agent_verify_key) {
        return false;
    }
    let Ok(sig_control_bytes) = <[u8; SIGNATURE_LEN]>::try_from(contresignature.sig_control.as_slice())
    else {
        return false;
    };
    control_verify_key
        .verify_strict(&receipt.signing_bytes(), &Signature::from_bytes(&sig_control_bytes))
        .is_ok()
}

/// `SHA-256(bytes)` — helper local (le crate audit n'expose pas de helper public).
fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&out);
    digest
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloison_audit::{Counters, ReceiptMessage};

    fn receipt() -> Receipt {
        Receipt::build(ReceiptMessage {
            tenant_id: "tenant-42".to_string(),
            session_ref_hashed: "abc".to_string(),
            ts_unix: 1_700_000_000,
            engine_version: "0.1.0".to_string(),
            policy_hash: "def".to_string(),
            counters: Counters::default(),
        })
    }

    #[test]
    fn contresignature_round_trip_on_real_receipt() {
        let agent_key = SigningKey::from_bytes(&[7u8; 32]);
        let control_key = SigningKey::from_bytes(&[8u8; 32]);

        // Un reçu réellement signé par l'agent (message = signing_bytes()) passe.
        let signed = receipt().sign(&agent_key);
        let cs = contresigner_reçu(&signed, &agent_key.verifying_key(), &control_key)
            .expect("un reçu signé par l'agent doit passer");
        assert_eq!(cs.sig_agent, signed.sig_agent);
        assert!(verifier_contresignature(
            &cs,
            &signed,
            &agent_key.verifying_key(),
            &control_key.verifying_key()
        ));

        // Mauvaise clé agent → refus, aucune contresignature.
        let wrong_agent = SigningKey::from_bytes(&[9u8; 32]);
        assert!(matches!(
            contresigner_reçu(&signed, &wrong_agent.verifying_key(), &control_key),
            Err(ControlError::InvalidAgentSignature)
        ));

        // Reçu non signé (sig_agent vide) → refus.
        assert!(matches!(
            contresigner_reçu(&receipt(), &agent_key.verifying_key(), &control_key),
            Err(ControlError::InvalidAgentSignature)
        ));

        // Reçu altéré après signature (même signature, contenu différent) → refus.
        let mut tampered = signed.clone();
        tampered.counters.incomplete_restorations += 1;
        assert!(matches!(
            contresigner_reçu(&tampered, &agent_key.verifying_key(), &control_key),
            Err(ControlError::InvalidAgentSignature)
        ));
    }
}
