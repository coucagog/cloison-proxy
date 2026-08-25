//! CLOISON — `cloison-wasm` : module navigateur **`@cloison/core`** (N0 v1.1,
//! chantier ③ — N0V11-PREP §2.3).
//!
//! Périmètre minimal : **tokenize/restore in-browser** (wasm-bindgen),
//! **coffre in-memory** (aucune valeur persistée en clair — le registre
//! d'émission vit dans la session WASM, charte §9.1), **zéro secret dans la
//! page**.
//!
//! Les bindings (session, tokenize, restore, detect, derive_keys) sont
//! définis dans `cloison-core::wasm` (gated `target_arch = "wasm32"`) — ce
//! crate les **ré-exporte** pour un packaging navigateur propre :
//!
//! ```bash
//! # Build du module navigateur (une fois par version) :
//! cd crates/cloison-wasm
//! wasm-pack build --target web --out-dir ../../deploy/wasm-demo/pkg
//! # → deploy/wasm-demo/pkg/ (glue JS + .wasm) ; page de démo :
//! #   deploy/wasm-demo/index.html
//! ```
//!
//! Invariants : la restauration reste bornée au registre de la requête (I3) ;
//! aucune valeur ne quitte la page ; le tenant key est fourni par l'appelant
//! (jamais embarqué dans le module).

#[cfg(target_arch = "wasm32")]
pub use cloison_core::wasm::*;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles_and_core_links() {
        // La ré-export est gated wasm32 ; sur natif, on vérifie que le
        // moteur est bien lié (surface minimale utilisable en test).
        use cloison_core::Policy;
        let _ = Policy::default();
    }
}
