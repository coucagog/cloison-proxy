//! Permissions de fichiers **portables** (Linux / macOS / Windows).
//!
//! Invariant (charte §2, docs/SECURITY.md) : les fichiers sensibles du proxy
//! (sel de session, clé d'audit Ed25519, journal des reçus, curseur d'ingest)
//! sont créés **0600** — jamais lisibles par un autre utilisateur.
//!
//! Sur Unix, le mode POSIX s'applique directement (`OpenOptionsExt::mode`).
//! Sur **Windows**, il n'existe pas de mode POSIX : les ACL du profil
//! utilisateur s'appliquent (le répertoire `~/.cloison` est restreint au
//! propriétaire par le système) — l'application est un **no-op documenté**
//! (portage Windows découvert par la CI release-n0, journal STACK-N0V13).

/// Marqueur de restriction : applique `mode` au fichier sur Unix, no-op sur
/// Windows (ACL système).
pub struct Restrict(u32);

/// Crée un marqueur de permissions POSIX à appliquer à un `OpenOptions`.
pub fn restrict(mode: u32) -> Restrict {
    Restrict(mode)
}

impl Restrict {
    /// Applique la restriction (Unix) ou ne fait rien (Windows).
    pub fn apply(self, opts: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(self.0);
        }
        #[cfg(not(unix))]
        {
            let _ = self.0;
        }
        opts
    }
}
