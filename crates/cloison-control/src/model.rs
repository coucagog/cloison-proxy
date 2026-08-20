//! Modèles du plan de contrôle — **aucune PII**.
//!
//! - [`Tenant`] : identifiant opérateur non sensible + `nom_public` opérateur +
//!   `tokens_version` (propagation des rotations/révocations vers les caches proxy) ;
//! - [`ApiToken`] : ne persiste **que** `token_hash` (SHA-256 du `mn_` clair) — le clair
//!   n'existe que dans la réponse d'émission (`TokenIssued`) ; l'ancien jeton reste
//!   utilisable pendant `grace_until` après une rotation ;
//! - [`License`] : quotas (jamais de données utilisateur) ;
//! - [`Policy`] : JSON de configuration opérateur — jamais de texte client.

use serde::{Deserialize, Serialize};

/// Locataire — identifiant opérateur, jamais un texte utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    /// Nom public opérateur (non sensible).
    pub nom_public: String,
    pub statut: TenantStatut,
    pub created_at: u64,
    /// Version des jetons du tenant : incrémentée à chaque rotation/révocation.
    /// Le proxy (voir `API_DESIGN.md` §2.4/§2.5) cache les vues de jetons et long-polle
    /// `GET /v1/control/version` (ETag) : toute montée de version purge les entrées
    /// de cache périmées → révocation quasi-instantanée même avec un cache local.
    pub tokens_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatut {
    Actif,
    Suspendu,
    Supprime,
}

/// Jeton d'accès `mn_`. Le stockage ne contient que le hash :
/// `token_hash = hex(SHA-256("cloison-mn-token-v1:" ‖ clair))`.
/// **Le clair n'est jamais persisté, jamais loggé, jamais envoyé sur le fil.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub tenant_id: String,
    /// hex(SHA-256(domaine ‖ clair)) — JAMAIS le clair.
    pub token_hash: String,
    pub scopes: Vec<String>,
    pub created_at: u64,
    /// Non nul après une rotation : l'ancien secret reste valide jusqu'à `grace_until`
    /// (période de grâce, `CLOISON_ROTATION_GRACE_SECONDS`, défaut 300 s).
    pub rotated_at: Option<u64>,
    /// Fin de la grâce après rotation (`rotated_at + grace_secs`) ; le jeton est invalide
    /// à partir de cet instant.
    pub grace_until: Option<u64>,
    /// Non nul après une révocation (invalidation immédiate, aucune grâce).
    pub revoked_at: Option<u64>,
}

impl ApiToken {
    /// Construit un jeton à partir du clair : seul le hash est conservé.
    /// Le clair doit être affiché **une seule fois** par l'appelant puis oublié.
    pub fn issue(
        id: String,
        tenant_id: String,
        token_clair: &str,
        scopes: Vec<String>,
        now_unix: u64,
    ) -> ApiToken {
        ApiToken {
            id,
            tenant_id,
            token_hash: crate::token::token_hash(token_clair),
            scopes,
            created_at: now_unix,
            rotated_at: None,
            grace_until: None,
            revoked_at: None,
        }
    }

    /// Vrai si le jeton est utilisable à l'instant `now_unix` :
    /// ni révoqué, ni roté, ou roté mais encore dans la période de grâce.
    pub fn is_active_at(&self, now_unix: u64) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match self.rotated_at {
            None => true,
            Some(_) => match self.grace_until {
                // Rotation : valide tant que la grâce n'est pas expirée.
                Some(until) => now_unix < until,
                None => false,
            },
        }
    }

    /// Vrai si le jeton est utilisable à l'instant courant (équivalent pratique
    /// de `is_active_at(now_unix())`).
    pub fn is_active(&self) -> bool {
        self.is_active_at(crate::now_unix())
    }
}

/// Réponse d'émission : **seul endroit** où le clair `mn_` apparaît.
///
/// Garde-fous anti-fuite (P1-5) :
/// - `Debug` masque le clair (`token: <redacted>`) — aucun log accidentel ne le révèle ;
/// - le trait `Serialize` ne sérialise **jamais** le clair, seulement `token_hash` :
///   une ré-émission accidentelle de l'objet ne peut pas fuiter le secret ;
/// - le clair n'est sérialisé que par [`TokenIssued::to_issued_json`], appelé
///   explicitement par les deux handlers d'émission (affichage unique, puis oubli).
#[derive(Clone, PartialEq, Eq)]
pub struct TokenIssued {
    pub id: String,
    /// Clair `mn_` — à afficher une fois, puis à ne plus jamais stocker ni logger.
    pub token: String,
    pub expires_at: Option<u64>,
}

impl TokenIssued {
    /// JSON de la réponse d'émission — **LE seul chemin de sérialisation du clair**,
    /// utilisé uniquement par `POST /admin/tenants/{id}/tokens` et `.../rotate`.
    pub fn to_issued_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "token": self.token,
            "expires_at": self.expires_at,
        })
    }

    /// `hex(SHA-256(domaine ‖ clair))` — le hash, jamais le clair.
    pub fn token_hash(&self) -> String {
        crate::token::token_hash(&self.token)
    }
}

impl std::fmt::Debug for TokenIssued {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenIssued")
            .field("id", &self.id)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Serialize for TokenIssued {
    /// Sérialise `{id, token_hash, expires_at}` — **jamais le clair**. Toute
    /// sérialisation accidentelle (logs, cache, test) ne peut exposer que le hash.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("TokenIssued", 3)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("token_hash", &self.token_hash())?;
        s.serialize_field("expires_at", &self.expires_at)?;
        s.end()
    }
}

/// Licence d'un locataire : quotas, jamais de données utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct License {
    pub tenant_id: String,
    pub plan: Plan,
    pub limites: LicenseLimites,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Free,
    Pro,
    Enterprise,
}

/// Quotas de licence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseLimites {
    pub max_requests_per_day: u64,
    pub max_tokens: u32,
}

impl Default for LicenseLimites {
    fn default() -> Self {
        LicenseLimites {
            max_requests_per_day: 1000,
            max_tokens: 16,
        }
    }
}

/// Politique opérateur (règles du détecteur) — **jamais du texte client**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub tenant_id: String,
    /// JSON canonique de la règle (configuration opérateur).
    pub json_policy: String,
    /// Incrémenté à chaque publication.
    pub version: u64,
    pub updated_at: u64,
}
