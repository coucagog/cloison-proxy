//! Authentification par clé composite : `Authorization: Bearer mn_<jeton>.<cle_amont>`.
//!
//! Découpage sur le **premier point** (`splitn(2, '.')`) : la clé amont peut
//! elle-même contenir des points — ils sont conservés tels quels. La clé amont
//! ne circule que dans le header `Authorization` de la requête amont et
//! n'apparaît jamais dans un log (`Debug` écrasé).

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::errors::{ErrorKind, ProxyError};
use crate::handlers::AppState;

/// Préfixe obligatoire du jeton d'accès.
pub const ACCESS_TOKEN_PREFIX: &str = "mn_";

/// Clé composite décodée : `mn_<jeton_acces>.<cle_amont>`.
///
/// `Debug` est volontairement **écrasé** : la clé amont n'apparaît jamais dans
/// un log (invariant I1). Seuls le jeton tronqué et la longueur de la clé sont
/// affichés.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct CompositeKey {
    /// Partie `mn_…` (avec le préfixe, pour vérification exacte).
    pub access_token: Zeroizing<String>,
    /// Secret amont — header amont uniquement.
    pub upstream_key: Zeroizing<String>,
}

impl std::fmt::Debug for CompositeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown: String = self.access_token.chars().take(6).collect();
        f.debug_struct("CompositeKey")
            .field("access_token", &format_args!("{shown}…"))
            .field("upstream_key_len", &self.upstream_key.len())
            .finish()
    }
}

/// Analyse le header `Authorization`.
///
/// Règles (dans l'ordre, chacune → 401 `invalid_api_key`) :
/// 1. header absent ou schéma ≠ `Bearer` (insensible à la casse) ;
/// 2. moins de 2 parties après `splitn(2, '.')` ;
/// 3. partie jeton ne commençant pas par `mn_` ;
/// 4. jeton vide au-delà du préfixe ;
/// 5. clé amont vide.
pub fn parse_authorization(header_value: &str) -> Result<CompositeKey, ProxyError> {
    let invalid = || ProxyError::new(ErrorKind::Auth, "invalid api key");

    let Some((scheme, rest)) = header_value.split_once(' ') else {
        return Err(invalid());
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(invalid());
    }
    let token = rest.trim();

    let mut parts = token.splitn(2, '.');
    let access_token = parts.next().unwrap_or("");
    let upstream_key = match parts.next() {
        Some(k) => k,
        None => return Err(invalid()),
    };

    if !access_token.starts_with(ACCESS_TOKEN_PREFIX) {
        return Err(invalid());
    }
    if access_token.len() == ACCESS_TOKEN_PREFIX.len() {
        return Err(invalid());
    }
    if upstream_key.is_empty() {
        return Err(invalid());
    }

    Ok(CompositeKey {
        access_token: Zeroizing::new(access_token.to_string()),
        upstream_key: Zeroizing::new(upstream_key.to_string()),
    })
}

/// Middleware axum : valide le header, insère `CompositeKey` dans les extensions.
/// Échec → 401 `invalid_api_key`, **aucun** appel amont n'est fait.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, ProxyError> {
    let fail = || {
        state
            .metrics
            .auth_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ProxyError::new(ErrorKind::Auth, "invalid api key")
    };

    let header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let Some(header) = header else {
        return Err(fail());
    };

    let key = match parse_authorization(&header) {
        Ok(k) => k,
        Err(_) => return Err(fail()),
    };

    // Deux modes d'auth (jamais les deux à la fois — le contrôle prime s'il est
    // configuré) :
    //   - wiring C (`CLOISON_CONTROL_URL` posé) : vérification par hash auprès du
    //     contrôle (TokenVerifier, cache local + purge sur rotation). Panne du
    //     contrôle et cache froid → fail-closed 401 (jamais d'acceptation par
    //     défaut, invariant I8).
    //   - N0/historique : comparaison à temps constant contre
    //     `CLOISON_EXPECTED_ACCESS_TOKEN` si configuré.
    if let Some(verifier) = &state.token_verifier {
        match verifier.verify(key.access_token.as_str()).await {
            Ok(true) => {}
            Ok(false) => return Err(fail()),
            Err(_) => return Err(fail()),
        }
    } else if let Some(expected) = &state.expected_access_token {
        let matches = key.access_token.len() == expected.len()
            && bool::from(key.access_token.as_bytes().ct_eq(expected.as_bytes()));
        if !matches {
            return Err(fail());
        }
    }

    req.extensions_mut().insert(key);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_composite_key_with_dotted_upstream_key() {
        let key = parse_authorization("Bearer mn_testtoken.sk-a.b.c").unwrap();
        assert_eq!(key.access_token.as_str(), "mn_testtoken");
        assert_eq!(key.upstream_key.as_str(), "sk-a.b.c");
    }

    #[test]
    fn rejects_malformed_headers() {
        for bad in [
            "",
            "Basic mn_x.sk",
            "Bearer",
            "Bearer foo",
            "Bearer mn_x",
            "Bearer mn_",
            "Bearer mn_x.",
            "Bearer .sk",
        ] {
            assert!(parse_authorization(bad).is_err(), "should reject: {bad:?}");
        }
    }

    #[test]
    fn debug_redacts_upstream_key() {
        let key = parse_authorization("Bearer mn_token.sk-super-secret").unwrap();
        let dbg = format!("{key:?}");
        assert!(
            !dbg.contains("sk-super-secret"),
            "upstream key leaked in Debug: {dbg}"
        );
        assert!(dbg.contains("upstream_key_len"), "{dbg}");
    }
}
