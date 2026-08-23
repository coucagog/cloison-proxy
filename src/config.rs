//! Configuration du proxy, lue depuis l'environnement (`CLOISON_*`).
//!
//! Variables :
//! - `CLOISON_LISTEN_ADDR` (défaut `0.0.0.0:8787`) ou `CLOISON_PROXY_PORT` (port seul) ;
//! - `CLOISON_UPSTREAM_BASE_URL` (requis hors mock) ;
//! - `CLOISON_UPSTREAM_CHAT_PATH` / `CLOISON_UPSTREAM_COMPLETIONS_PATH` /
//!   `CLOISON_UPSTREAM_MODELS_PATH` (défauts `/v1/chat/completions`, `/v1/completions`, `/v1/models`) ;
//! - `CLOISON_UPSTREAM_CONNECT_TIMEOUT_MS` (défaut 5000), `CLOISON_UPSTREAM_TIMEOUT_MS` (défaut 30000) ;
//! - `CLOISON_MAX_BODY_BYTES` (défaut 1 MiB) ;
//! - `CLOISON_STREAM_MAX_TOKEN_LEN` (défaut 64, plafonné à 256), `CLOISON_STREAM_NEUTRAL_MARKER`
//!   (défaut `[REDACTED]`), `CLOISON_STREAM_KEEP_ALIVE_MS` (défaut 15000) ;
//! - `CLOISON_EXPECTED_ACCESS_TOKEN` (optionnel, comparé à temps constant) ;
//! - `CLOISON_TENANT_KEY_HEX` (64 hex — requis hors mock), `CLOISON_SESSION_SALT_HEX`
//!   (32 hex — aléatoire par boot si absent) ;
//! - `CLOISON_MOCK_MODE` (`1`/`true` : assouplit les prérequis, clé locataire de dev) ;
//! - `CLOISON_DETECT_URL` (optionnel — URL REST `POST /detect` du sidecar NER,
//!   wiring B.1), `CLOISON_DETECT_TIMEOUT_MS` (défaut 2000).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;
use zeroize::Zeroizing;

use crate::errors::{ErrorKind, ProxyError};

/// Adresse d'écoute par défaut.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:8787";
/// Chemins OpenAI par défaut.
pub const DEFAULT_CHAT_PATH: &str = "/v1/chat/completions";
pub const DEFAULT_COMPLETIONS_PATH: &str = "/v1/completions";
pub const DEFAULT_MODELS_PATH: &str = "/v1/models";
/// Limite de corps par défaut : 1 MiB.
pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
/// Taille max d'une sentinelle CLOISON (≈ 31–38 octets UTF-8) ; borne aussi le tampon de flux.
pub const DEFAULT_STREAM_MAX_TOKEN_LEN: usize = 64;
/// Plafond dur de `max_token_len`.
pub const STREAM_MAX_TOKEN_LEN_CAP: usize = 256;
/// Marqueur neutre par défaut (fail-loud).
pub const DEFAULT_NEUTRAL_MARKER: &str = "[REDACTED]";
/// Timeouts amont par défaut (ms).
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
/// Intervalle de keep-alive SSE par défaut (ms).
pub const DEFAULT_KEEP_ALIVE_MS: u64 = 15_000;
/// Seuil k-anonyme du rapport de conformité par défaut (STACK-4).
pub const DEFAULT_AUDIT_K: usize = 5;
/// Timeout de la requête detect par défaut (ms) — B.1.
pub const DEFAULT_DETECT_TIMEOUT_MS: u64 = 2_000;

/// Configuration complète du proxy.
#[derive(Clone)]
pub struct Config {
    /// Adresse d'écoute du proxy.
    pub listen_addr: SocketAddr,
    /// Configuration de l'amont (fournisseur LLM).
    pub upstream: UpstreamConfig,
    /// Configuration du flux SSE (buffer-and-scan).
    pub stream: StreamConfig,
    /// Jeton d'accès attendu (optionnel) : si présent, validation à temps constant.
    pub expected_access_token: Option<Zeroizing<String>>,
    /// Clé locataire 32 octets → `SessionKeys::derive`.
    pub tenant_key: [u8; 32],
    /// Sel de session 16 octets ; aléatoire par boot si absent (rotation).
    pub session_salt: [u8; 16],
    /// Mode mock : assouplit `CLOISON_UPSTREAM_BASE_URL` / `CLOISON_TENANT_KEY_HEX`
    /// (clé de développement fixe) — jamais activé en production.
    pub mock_mode: bool,
    /// Mode audit observe-only (`CLOISON_AUDIT_MODE=1`) : le proxy détecte et
    /// **compte** les PII sans rien masquer, produit un reçu signé par requête
    /// et un rapport de conformité k-anonyme. Défaut : désactivé.
    pub audit_mode: bool,
    /// Chemin vers la clé de signature Ed25519 de l'agent au bord
    /// (`CLOISON_AUDIT_KEYS`) : 32 octets bruts ou 64 hex. Si le fichier
    /// n'existe pas, une clé est générée et écrite (0600) au boot.
    pub audit_keys: Option<PathBuf>,
    /// Seuil k-anonyme du rapport de conformité (`CLOISON_AUDIT_K`, défaut 5).
    pub audit_k: usize,
    /// Persistance des reçus d'audit (`CLOISON_AUDIT_LEDGER_FILE`) : JSONL
    /// append-only 0600, rechargé au boot. `None` = journal en mémoire seule
    /// (perte au restart, dégradé).
    pub audit_ledger_file: Option<PathBuf>,
    /// Wiring edge → sidecar detect (B.1) : URL REST du sidecar NER
    /// (`CLOISON_DETECT_URL`, ex. `http://detect:8080/detect`). `None` =
    /// détection embarquée seule (comportement historique).
    pub detect: DetectConfig,
}

/// Configuration du sidecar `cloison-detect` (wiring edge→detect, B.1).
#[derive(Debug, Clone)]
pub struct DetectConfig {
    /// URL complète du endpoint REST `POST /detect` ; `None` = non câblé.
    pub url: Option<Url>,
    /// Timeout de la requête detect (ms) — au-delà, dégradation gracieuse
    /// (détection embarquée seule), jamais de blocage du proxy.
    pub timeout: Duration,
}

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            url: None,
            timeout: Duration::from_millis(DEFAULT_DETECT_TIMEOUT_MS),
        }
    }
}

impl std::fmt::Debug for Config {
    /// `Debug` volontairement rougeoyé : le jeton attendu n'apparaît jamais en clair.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("listen_addr", &self.listen_addr)
            .field("upstream", &self.upstream)
            .field("stream", &self.stream)
            .field(
                "expected_access_token",
                &self
                    .expected_access_token
                    .as_ref()
                    .map(|t| format!("{}…", t.chars().take(6).collect::<String>())),
            )
            .field("tenant_key_set", &(self.tenant_key != [0u8; 32]))
            .field("session_salt", &hex(&self.session_salt))
            .field("mock_mode", &self.mock_mode)
            .field("audit_mode", &self.audit_mode)
            .field("audit_keys", &self.audit_keys)
            .field("audit_k", &self.audit_k)
            .field("audit_ledger_file", &self.audit_ledger_file)
            .field("detect_url", &self.detect.url)
            .field("detect_timeout", &self.detect.timeout)
            .finish_non_exhaustive()
    }
}

/// Configuration de l'amont (fournisseur LLM). Ne contient JAMAIS de secret.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// URL de base du fournisseur. Configurable → test contre un mock echo.
    pub base_url: Url,
    /// Chemin `chat/completions` (défaut `/v1/chat/completions`).
    pub chat_completions_path: String,
    /// Chemin `completions` legacy (défaut `/v1/completions`).
    pub completions_path: String,
    /// Chemin `models` (défaut `/v1/models`).
    pub models_path: String,
    /// Timeout de connexion amont.
    pub connect_timeout: Duration,
    /// Timeout global de requête amont.
    pub request_timeout: Duration,
    /// Limite de corps de requête entrante (défaut 1 MiB).
    pub max_body_bytes: usize,
}

/// Configuration du flux SSE.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Taille max d'une sentinelle CLOISON ; borne aussi le tampon (`max_token_len - 1`).
    pub max_token_len: usize,
    /// Marqueur neutre émis quand un jeton n'est pas résoluble (fail-loud).
    pub neutral_marker: String,
    /// Intervalle du keep-alive SSE.
    pub keep_alive: Duration,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_token_len: DEFAULT_STREAM_MAX_TOKEN_LEN,
            neutral_marker: DEFAULT_NEUTRAL_MARKER.to_string(),
            keep_alive: Duration::from_millis(DEFAULT_KEEP_ALIVE_MS),
        }
    }
}

/// Charge la configuration depuis l'environnement.
///
/// Toute valeur manquante ou invalide est une erreur **fatale au boot**.
pub fn load() -> Result<Config, ProxyError> {
    let mock_mode = env_bool("CLOISON_MOCK_MODE")?;

    let listen_addr: SocketAddr = match std::env::var("CLOISON_LISTEN_ADDR") {
        Ok(v) => v.parse::<SocketAddr>().map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid CLOISON_LISTEN_ADDR").with_field("detail", e.to_string())
        })?,
        Err(_) => match std::env::var("CLOISON_PROXY_PORT") {
            Ok(port) => format!("0.0.0.0:{port}").parse::<SocketAddr>().map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "invalid CLOISON_PROXY_PORT").with_field("detail", e.to_string())
            })?,
            Err(_) => DEFAULT_LISTEN_ADDR.parse().expect("static default address"),
        },
    };

    let base_url = match std::env::var("CLOISON_UPSTREAM_BASE_URL") {
        Ok(v) => Url::parse(&v).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid CLOISON_UPSTREAM_BASE_URL").with_field("detail", e.to_string())
        })?,
        Err(_) if mock_mode => Url::parse("http://127.0.0.1:1")
            .expect("static placeholder URL")
            // En mode mock sans URL, tout appel amont échoue immédiatement et
            // bruyamment — le test doit toujours pointer un vrai mock.
            ,
        Err(_) => {
            return Err(ProxyError::new(
                ErrorKind::Internal,
                "CLOISON_UPSTREAM_BASE_URL is required (or enable CLOISON_MOCK_MODE)",
            ));
        }
    };

    let tenant_key: [u8; 32] = match std::env::var("CLOISON_TENANT_KEY_HEX") {
        Ok(v) => decode_hex(&v).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid CLOISON_TENANT_KEY_HEX (64 hex chars required)")
                .with_field("detail", e.to_string())
        })?,
        Err(_) if mock_mode => [0x42; 32],
        Err(_) => {
            return Err(ProxyError::new(
                ErrorKind::Internal,
                "CLOISON_TENANT_KEY_HEX is required (64 hex chars) unless CLOISON_MOCK_MODE=1",
            ));
        }
    };

    let session_salt: [u8; 16] = match std::env::var("CLOISON_SESSION_SALT_HEX") {
        // Chaîne vide = non configuré (le compose passe ) : sel aléatoire par boot.
        Ok(v) if !v.is_empty() => decode_hex(&v).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid CLOISON_SESSION_SALT_HEX (32 hex chars required)")
                .with_field("detail", e.to_string())
        })?,
        _ => random_salt(),
    };

    let expected_access_token = match std::env::var("CLOISON_EXPECTED_ACCESS_TOKEN") {
        Ok(v) if !v.is_empty() => Some(Zeroizing::new(v)),
        _ => None,
    };

    let upstream = UpstreamConfig {
        base_url,
        chat_completions_path: env("CLOISON_UPSTREAM_CHAT_PATH", DEFAULT_CHAT_PATH),
        completions_path: env("CLOISON_UPSTREAM_COMPLETIONS_PATH", DEFAULT_COMPLETIONS_PATH),
        models_path: env("CLOISON_UPSTREAM_MODELS_PATH", DEFAULT_MODELS_PATH),
        connect_timeout: Duration::from_millis(env_u64("CLOISON_UPSTREAM_CONNECT_TIMEOUT_MS", DEFAULT_CONNECT_TIMEOUT_MS)?),
        request_timeout: Duration::from_millis(env_u64("CLOISON_UPSTREAM_TIMEOUT_MS", DEFAULT_REQUEST_TIMEOUT_MS)?),
        max_body_bytes: env_usize("CLOISON_MAX_BODY_BYTES", DEFAULT_MAX_BODY_BYTES)?,
    };

    let stream = StreamConfig {
        max_token_len: env_usize("CLOISON_STREAM_MAX_TOKEN_LEN", DEFAULT_STREAM_MAX_TOKEN_LEN)?
            .clamp(1, STREAM_MAX_TOKEN_LEN_CAP),
        neutral_marker: env("CLOISON_STREAM_NEUTRAL_MARKER", DEFAULT_NEUTRAL_MARKER),
        keep_alive: Duration::from_millis(env_u64("CLOISON_STREAM_KEEP_ALIVE_MS", DEFAULT_KEEP_ALIVE_MS)?),
    };

    // STACK-4 : mode audit observe-only (défaut : désactivé — aucun reçu,
    // aucun changement de comportement par rapport à STACK-3).
    let audit_mode = env_bool("CLOISON_AUDIT_MODE")?;
    let audit_keys = match env("CLOISON_AUDIT_KEYS", "") {
        s if s.is_empty() => None,
        s => Some(PathBuf::from(s)),
    };
    let audit_k = env_usize("CLOISON_AUDIT_K", DEFAULT_AUDIT_K)?.max(2);
    let audit_ledger_file = match env("CLOISON_AUDIT_LEDGER_FILE", "") {
        s if s.is_empty() => None,
        s => Some(PathBuf::from(s)),
    };

    // B.1 — wiring edge → detect : URL REST du sidecar NER (optionnel).
    let detect = DetectConfig {
        url: match env("CLOISON_DETECT_URL", "") {
            s if s.is_empty() => None,
            s => Some(Url::parse(&s).map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "invalid CLOISON_DETECT_URL").with_field("detail", e.to_string())
            })?),
        },
        timeout: Duration::from_millis(env_u64("CLOISON_DETECT_TIMEOUT_MS", DEFAULT_DETECT_TIMEOUT_MS)?),
    };

    Ok(Config {
        listen_addr,
        upstream,
        stream,
        expected_access_token,
        tenant_key,
        session_salt,
        mock_mode,
        audit_mode,
        audit_keys,
        audit_k,
        audit_ledger_file,
        detect,
    })
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_bool(name: &str) -> Result<bool, ProxyError> {
    match std::env::var(name) {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            other => Err(ProxyError::new(
                ErrorKind::Internal,
                format!("invalid boolean for {name}: {other}"),
            )),
        },
        Err(_) => Ok(false),
    }
}

fn env_usize(name: &str, default: usize) -> Result<usize, ProxyError> {
    match std::env::var(name) {
        Ok(v) => v.parse::<usize>().map_err(|e| {
            ProxyError::new(ErrorKind::Internal, format!("invalid integer for {name}: {v}")).with_field("detail", e.to_string())
        }),
        Err(_) => Ok(default),
    }
}

fn env_u64(name: &str, default: u64) -> Result<u64, ProxyError> {
    match std::env::var(name) {
        Ok(v) => v.parse::<u64>().map_err(|e| {
            ProxyError::new(ErrorKind::Internal, format!("invalid integer for {name}: {v}")).with_field("detail", e.to_string())
        }),
        Err(_) => Ok(default),
    }
}

/// Décode une chaîne hexadécimale en `[u8; N]` (taille exacte imposée).
fn decode_hex<const N: usize>(input: &str) -> Result<[u8; N], ProxyError> {
    if input.len() != N * 2 {
        return Err(ProxyError::new(
            ErrorKind::Internal,
            format!("expected {} hex chars, got {}", N * 2, input.len()),
        ));
    }
    let bytes = input.as_bytes();
    let mut out = [0u8; N];
    for (i, chunk) in bytes.chunks(2).enumerate() {
        let hi = hex_val(chunk[0]).ok_or_else(|| ProxyError::new(ErrorKind::Internal, "invalid hex digit"))?;
        let lo = hex_val(chunk[1]).ok_or_else(|| ProxyError::new(ErrorKind::Internal, "invalid hex digit"))?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sel de session aléatoire : rotation des jetons à chaque boot (invariant I7).
fn random_salt() -> [u8; 16] {
    use rand::Rng;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_roundtrip() {
        let k = decode_hex::<32>(&"ab".repeat(32)).unwrap();
        assert_eq!(k, [0xab; 32]);
        assert!(decode_hex::<32>("ab").is_err());
        assert!(decode_hex::<16>("zz".repeat(16).as_str()).is_err());
    }
}
