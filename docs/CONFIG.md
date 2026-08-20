# CLOISON — Configuration (`CLOISON_*`)

> Référence générée depuis le code : `crates/cloison-proxy/src/config.rs`
> (edge), `crates/cloison-control/src/main.rs` + `api.rs` (control),
> `services/cloison-detect/src/config.py` (detect, pydantic).
> Colonnes : **Rôle** (edge | control | detect | ops), **Secret** (oui/non),
> **Défaut**. Les variables marquées *(STACK-7)* sont **ciblées mais non
> encore lues** par le binaire actuel — leur présence est sans effet
> aujourd'hui.

## 1. Edge — `cloison-proxy` (binaire Rust)

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `CLOISON_LISTEN_ADDR` | edge | non | `0.0.0.0:8787` | adresse d'écoute complète |
| `CLOISON_PROXY_PORT` | edge | non | — | port seul (alternative à `CLOISON_LISTEN_ADDR`) |
| `CLOISON_UPSTREAM_BASE_URL` | edge | non | **requis** (hors mock) | URL de base du fournisseur LLM |
| `CLOISON_UPSTREAM_CHAT_PATH` | edge | non | `/v1/chat/completions` | chemin `chat/completions` |
| `CLOISON_UPSTREAM_COMPLETIONS_PATH` | edge | non | `/v1/completions` | chemin legacy `completions` |
| `CLOISON_UPSTREAM_MODELS_PATH` | edge | non | `/v1/models` | chemin `models` |
| `CLOISON_UPSTREAM_CONNECT_TIMEOUT_MS` | edge | non | `5000` | timeout de connexion amont |
| `CLOISON_UPSTREAM_TIMEOUT_MS` | edge | non | `30000` | timeout global amont |
| `CLOISON_MAX_BODY_BYTES` | edge | non | `1048576` (1 MiB) | limite de corps entrante |
| `CLOISON_STREAM_MAX_TOKEN_LEN` | edge | non | `64` (plafond `256`) | taille max d'une sentinelle / tampon flux |
| `CLOISON_STREAM_NEUTRAL_MARKER` | edge | non | `[REDACTED]` | marqueur fail-loud (jeton non résolu) |
| `CLOISON_STREAM_KEEP_ALIVE_MS` | edge | non | `15000` | intervalle keep-alive SSE |
| `CLOISON_EXPECTED_ACCESS_TOKEN` | edge | **oui** | absent (auth optionnelle) | jeton local `mn_*` attendu, comparé à temps constant |
| `CLOISON_TENANT_KEY_HEX` | edge/control | **oui** | **requis** (hors mock) | clé locataire 32 octets en hex (64 caractères) |
| `CLOISON_SESSION_SALT_HEX` | edge | **oui** | aléatoire par boot | sel de session 16 octets (32 hex) ; rotation des jetons |
| `CLOISON_MOCK_MODE` | edge | non | `0` | `1` = prérequis assouplis (clé de dev) — jamais en prod |
| `CLOISON_AUDIT_MODE` | edge | non | `0` | `1` = observe-only (reçus signés, rapport k-anonyme) |
| `CLOISON_AUDIT_KEYS` | edge | **oui** | absent (clé générée 0600) | chemin clé Ed25519 de l'agent (32 o bruts ou 64 hex) |
| `CLOISON_AUDIT_K` | edge | non | `5` | seuil k-anonyme (plancher 2) |
| `CLOISON_AUDIT_LEDGER_FILE` | edge | non | absent (mémoire seule) | persistance des reçus d'audit en JSONL append-only **0600**, rechargé au boot (survit au restart) |
| `RUST_LOG` | edge/control | non | `cloison_proxy=info` | filtre tracing (crate = `cloison_proxy`) |

## 2. Control — plan de contrôle (`cloison-control`)

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `CLOISON_ROLE` *(STACK-7)* | edge/control | non | `edge` | rôle du binaire unique (`edge` \| `control`) — dispatch à implémenter dans `config.rs` |
| `CLOISON_CONTROL_PORT` | control | non | `8788` | port d'écoute du plan de contrôle (le binaire ne lit PAS `CLOISON_LISTEN_ADDR`) |
| `CLOISON_LEDGER_FILE` | control | non | absent (mémoire) | journal append-only (JSONL, chaîne + signatures) ; posé → persistance |
| `CLOISON_ROTATION_GRACE_SECONDS` | control | non | `300` | grâce de rotation des jetons `mn_*` |
| `CLOISON_AGENT_VERIFY_KEY` | control | **oui** | absent (paire éphémère dev) | clé publique Ed25519 de l'agent, hex 64 |
| `CLOISON_CONTROL_SIGNING_KEY` | control | **oui** | absent (clé générée dev) | clé privée Ed25519 du contrôle, hex 64 |

## 3. Detect — `cloison-detect` (Python, pydantic)

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `CLOISON_GRPC_PORT` | detect | non | `50051` | port du transport gRPC (nominal) |
| `CLOISON_REST_PORT` | detect | non | `8080` | port REST (repli ; **note : la spec STACK-7 mentionnait 8789 — le code écoute sur 8080**, surchargeable ici) |
| `CLOISON_TRANSPORT` | detect | non | `rest` | `rest` \| `grpc` \| `both` |
| `CLOISON_OFFLINE` | detect | non | `0` | `1` = aucun téléchargement réseau |
| `CLOISON_PRELOAD` | detect | non | `auto` | `none` \| `auto` (presidio) \| `all` |
| `CLOISON_SPACY_SIZE` | detect | non | `md` | `sm` \| `md` \| `lg` (`fr_core_news_*`) — défaut `md` depuis le verdict GO (fr_sm hallucine PERSON/LOC) |
| `CLOISON_MODEL_CACHE_GB` | detect | non | `6.0` | cache modèles (Go) |
| `CLOISON_MODEL_DIR` | detect | non | `./models` | répertoire des modèles lourds (GLiNER…) — volume `/models` |
| `CLOISON_BUDGET_SECONDS` | detect | non | `2.0` | deadline douce par requête |
| `CLOISON_QUARANTINE_SECONDS` | detect | non | `300.0` | pas de rechargement après crash |
| `CLOISON_SESSION_MENTIONS_MAX` | detect | non | `200` | borne documentaire (côté core) |
| `CLOISON_ONNX` | detect | non | `0` | bascule runtime ONNX |
| `CLOISON_LOG_LEVEL` | detect | non | `INFO` | niveau de log |
| `CLOISON_CONSENSUS_PERSON_LOC` | detect | non | `1` | `1` = span PERSON/LOC mono-source < 0.90 refusé à la fusion (spécificité ; exempté `recall_only`) |
| `CLOISON_AFRICAN_MODEL` *(via `CLOISON_PRELOAD=all` ou config)* | detect | non | `afroxlmr` | NER ouest-africain : `serengeti` \| `afroxlmr` \| `masakha` — défaut `afroxlmr` (MasakhaNER 1+2, vrai NER) depuis le verdict GO ; `serengeti` est un LM sans tête NER (inutilisable), `masakha` est gated (401) |

Configurations imbriquées (non exposées en env, réglables dans le code /
`config.py`) : seuils par détecteur (`presidio_person: 0.45`, `presidio_loc:
0.40`, `spacy: 0.50`, `gliner: 0.45`, `serengeti: 0.50`), poids d'ensemble
(`presidio: 1.0`, `spacy: 0.8`, `gliner: 1.0`, `serengeti: 1.1`,
`afro: 1.1`), gazetteers LOC/PERSON par défaut, règles d'alias (cap 8
formes, score ≤ 0.85 du canonique).

## 4. Ops / compose

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `OPENROUTER_API_KEY` | ops | **oui** | — | clé fournisseur (partie amont de la clé composite) |
| `CLOISON_ACCESS_TOKEN` | ops | **oui** | — | alias du jeton local (partie `mn_*`) pour le compose |
| `CLOISON_PG_PASSWORD` | ops | **oui** | `cloison-dev-only` | mot de passe du miroir postgres (profil `db`, dev) |
| `CLOISON_DETECT_URL` *(STACK-7)* | ops | non | — | URL REST du sidecar consommée par le core — **non lu par le binaire actuel** |

## 5. Compatibilité fournisseurs (chemins amont)

| Fournisseur | `CLOISON_UPSTREAM_BASE_URL` | `CLOISON_UPSTREAM_CHAT_PATH` | `CLOISON_UPSTREAM_MODELS_PATH` |
|---|---|---|---|
| OpenRouter | `https://openrouter.ai/api/v1` | `/chat/completions` | `/models` |
| OpenRouter (style OpenAI) | `https://openrouter.ai/api` | `/v1/chat/completions` | `/v1/models` |
| DeepSeek | `https://api.deepseek.com` | `/chat/completions` | `/models` |

> ✅ Pré-requis HTTPS : `reqwest` est compilé avec `rustls-tls`
> (cf. `docs/DEPLOY.md` §1).

## 6. Règles de validation (échec fatal au boot)

- Booléens : `1/true/yes/on` ou `0/false/no/off` (sinon erreur) ;
- hex : longueur exacte (`CLOISON_TENANT_KEY_HEX` 64, `CLOISON_SESSION_SALT_HEX` 32) ;
- `CLOISON_UPSTREAM_BASE_URL` et `CLOISON_TENANT_KEY_HEX` **requis** hors `CLOISON_MOCK_MODE=1` ;
- detect (pydantic) : `transport`, `preload`, `spacy_size` validés ;
  valeurs invalides → fail-fast au démarrage.
