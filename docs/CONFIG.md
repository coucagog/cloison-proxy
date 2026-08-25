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
| `CLOISON_SESSION_SALT_HEX` | edge | **oui** | aléatoire par boot ; **fichier persistant en mode N0** | sel de session 16 octets (32 hex) ; rotation des jetons. En mode N0 (`CLOISON_VAULT_PATH` posé), le sel est **persisté** dans `<vault_path>.salt` (0600) si non fourni — la session du daemon survit aux redémarrages (la rotation reste possible en supprimant le fichier ou en posant la variable) |
| `CLOISON_VAULT_PATH` *(N0)* | edge | non | absent (pas de coffre) | **posé = mode N0 (daemon desktop)** : coffre redb **persistant** chiffré AES-256-GCM (`Vault`), clé dérivée de `CLOISON_VAULT_PASSPHRASE`, politique N0 (généralisation des faibles cardinalités explicite), sel de session persistant. Absent = comportement historique (pas de coffre, sel aléatoire par boot) |
| `CLOISON_VAULT_PASSPHRASE` *(N0)* | edge | **oui** | — | passphrase locale → clé du coffre (HKDF, **jamais persistée ni loggée**). **Requis si `CLOISON_VAULT_PATH` est posé** — absent → refus de démarrer (fail-loud) ; mauvaise passphrase sur un coffre existant → refus de démarrer (jamais de recréation silencieuse) |
| `CLOISON_VAULT_TTL_SECS` *(N0)* | edge | non | `604800` (7 j) | TTL des entrées du coffre (purge par session) |
| `CLOISON_SESSION_SALT_FILE` *(N0)* | edge | non | `<vault_path>.salt` | chemin explicite du fichier de sel persistant |
| `CLOISON_VAULT_KEYCHAIN_SERVICE` *(N0 v1.1 ②)* | edge | non | absent (env) | **posé = passphrase du coffre via le keychain OS** (Windows Credential Manager / macOS Keychain / Linux Secret Service-keyutils) — stockée **chiffrée par l'OS**, jamais en clair par CLOISON. Premier démarrage : l'env `CLOISON_VAULT_PASSPHRASE` est stockée dans le keychain ; ensuite le keychain fait foi. Keychain indisponible → repli env avec warn ; ni keychain ni env → **refus de démarrer** (fail-loud) |
| `CLOISON_VAULT_KEYCHAIN_USER` *(N0 v1.1 ②)* | edge | non | `default` | compte keychain du service |
| `CLOISON_ALIAS_EXPANSION` *(N0 v1.1)* | edge | non | `1` | **alias intra-session in-core** (R1–R7 : prénom seul, titre+nom, nom seul hors noms communs, diminutifs, raccourcis de lieux, casse/diacritiques) — jamais les pronoms, scores plafonnés. Actif en mode N0 (`CLOISON_VAULT_PATH` posé) |
| `CLOISON_QUASI_ID_GAUGE` *(N0 v1.1)* | edge | non | `0` | **jauge quasi-id in-core** (densité âge+acte+date+lieu, fenêtre glissante) — **opt-in** ; signal (compteur + log), jamais de résolution (charte §6.1 couche 6, §11) |
| `CLOISON_QUASI_ID_THRESHOLD` *(N0 v1.1)* | edge | non | `0.5` | seuil de la jauge (`score > seuil` strict ; `1.0` = désactivée de fait) |
| `CLOISON_ALIAS_MAX_MENTIONS` *(N0 v1.1)* | edge | non | `200` | borne documentaire du nombre de mentions canoniques en session (FIFO — miroir de `session_mentions_max` du sidecar) |
| `CLOISON_NER_MODEL_ONNX` *(N0 v1.2 ④)* | edge | non | absent | **chemin du modèle ONNX int8 du NER léger embarqué** (ex. `distilbert-base-multilingual-cased-ner-hrl` exporté + quantisé — 135 Mo, licence AFL-3.0 provisionnée, jamais committé). Posé **avec** `CLOISON_NER_TOKENIZER` ET en mode N0 (`CLOISON_VAULT_PATH`) → le daemon détecte PERSON/LOC **in-core** (jamais un sidecar Python). Modèle/lib absents → **dégradation gracieuse** (N0 v1 inchangé : gazetteers + alias), warn, jamais d'erreur (ARBITRAGE-04) |
| `CLOISON_NER_TOKENIZER` *(N0 v1.2 ④)* | edge | non | absent | chemin du `tokenizer.json` HF du NER léger (obligatoire avec `CLOISON_NER_MODEL_ONNX`) |
| `CLOISON_ONNX_LIB` *(N0 v1.2 ④)* | edge | non | `libonnxruntime.so` | chemin de la lib onnxruntime chargée dynamiquement (provisionnée avec le daemon — jamais embarquée dans le binaire) |
| `CLOISON_NER_THRESHOLD` *(N0 v1.2 ④)* | edge | non | `0.70` | seuil de score minimal du NER léger (balayage ARBITRAGE-04 : 0.70 = meilleur F1 PERSON/LOC) |
| `CLOISON_MOCK_MODE` | edge | non | `0` | `1` = prérequis assouplis (clé de dev) — jamais en prod |
| `CLOISON_AUDIT_MODE` | edge | non | `0` | `1` = observe-only (reçus signés, rapport k-anonyme) |
| `CLOISON_AUDIT_KEYS` | edge | **oui** | absent (clé générée 0600) | chemin clé Ed25519 de l'agent (32 o bruts ou 64 hex) |
| `CLOISON_AUDIT_K` | edge | non | `5` | seuil k-anonyme (plancher 2) |
| `CLOISON_AUDIT_LEDGER_FILE` | edge | non | absent (mémoire seule) | persistance des reçus d'audit en JSONL append-only **0600**, rechargé au boot (survit au restart) |
| `CLOISON_TENANT_ID` *(C)* | edge | non | `default` | locataire porté par les reçus d'audit et les vérifications de jeton (doit correspondre au tenant provisionné dans le contrôle) |
| `CLOISON_CONTROL_URL` *(C)* | edge | non | absent (N0) | URL de base du plan de contrôle — posée → **auth par hash** (`POST /v1/control/verify`), **ingest automatique** des reçus d'audit (`POST /v1/control/ingest`) et **long-poll** `GET /v1/control/version` (rotation). Absente → auth locale statique (`CLOISON_EXPECTED_ACCESS_TOKEN`), pas d'ingest |
| `CLOISON_CONTROL_INGEST_INTERVAL_SECS` *(C)* | edge | non | `60` | intervalle de flush des reçus d'audit vers le contrôle |
| `CLOISON_CONTROL_POLL_INTERVAL_SECS` *(C)* | edge | non | `30` | intervalle de long-poll des versions (rotation/révocation → purge du cache de jetons) |
| `CLOISON_CONTROL_VERIFY_CACHE_TTL_SECS` *(C)* | edge | non | `300` | TTL des décisions de vérification mises en cache (tolérance de panne du contrôle) |
| `RUST_LOG` | edge/control | non | `cloison_proxy=info` | filtre tracing (crate = `cloison_proxy`) |

## 2. Control — plan de contrôle (`cloison-control`)

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `CLOISON_ROLE` *(DEPLOY-10)* | edge/control | non | natif | rôle attendu du binaire (`edge` → cloison-proxy, `control` → cloison-control) — **LU au boot** : une valeur incompatible échoue bruyamment. Deux binaires distincts (le contrôle exige la feature `pg` que l'edge ne doit pas embarquer) ; absent = rôle natif (dev) |
| `CLOISON_CONTROL_PORT` | control | non | `8788` | port d'écoute du plan de contrôle (le binaire ne lit PAS `CLOISON_LISTEN_ADDR`) |
| `CLOISON_LEDGER_FILE` | control | non | absent (mémoire) | journal append-only (JSONL, chaîne + signatures) ; posé → persistance |
| `CLOISON_ROTATION_GRACE_SECONDS` | control | non | `300` | grâce de rotation des jetons `mn_*` |
| `CLOISON_DATABASE_URL` | control | **oui** | absent (InMemoryStore) | URL PostgreSQL — active `PostgresStore` (feature `pg` ; sinon repli mémoire + warning) |
| `CLOISON_AGENT_VERIFY_KEY` | control | **oui** | absent (paire éphémère dev) | clé publique Ed25519 de l'agent, hex 64 |
| `CLOISON_CONTROL_SIGNING_KEY` | control | **oui** | absent (clé générée dev) | clé privée Ed25519 du contrôle, hex 64 |

## 3. Detect — `cloison-detect` (Python, pydantic)

| Variable | Rôle | Secret | Défaut | Description |
|---|---|---|---|---|
| `CLOISON_GRPC_PORT` | detect | non | `50051` | port du transport gRPC (nominal) |
| `CLOISON_REST_PORT` | detect | non | `8080` | port REST (repli ; **note : la spec STACK-7 mentionnait 8789 — le code écoute sur 8080**, surchargeable ici) |
| `CLOISON_TRANSPORT` | detect | non | `rest` | `rest` \| `grpc` \| `both` |
| `CLOISON_OFFLINE` | detect | non | `0` | `1` = aucun téléchargement réseau |
| `CLOISON_PRELOAD` | detect | non | `auto` | `none` \| `auto` (presidio) \| `all` — **effectif au boot depuis B.2** (les modèles sont chargés avant de servir, pas au premier appel) |
| `CLOISON_SPACY_SIZE` | detect | non | `md` | `sm` \| `md` \| `lg` (`fr_core_news_*`) — défaut `md` depuis le verdict GO (fr_sm hallucine PERSON/LOC) |
| `CLOISON_MODEL_CACHE_GB` | detect | non | `6.0` | cache modèles (Go) |
| `CLOISON_MODEL_DIR` | detect | non | `./models` | répertoire des modèles lourds (GLiNER…) — volume `/models` |
| `CLOISON_BUDGET_SECONDS` | detect | non | `2.0` | deadline douce par requête |
| `CLOISON_QUARANTINE_SECONDS` | detect | non | `300.0` | pas de rechargement après crash |
| `CLOISON_SESSION_MENTIONS_MAX` | detect | non | `200` | borne documentaire (côté core) |
| `CLOISON_ONNX` | detect | non | `0` | `1` = inférence du NER africain (afroxlmr) via **ONNX Runtime** (CPU, int8 dynamique) au lieu de torch — gain ×2-3 attendu sur les docs longs (dette ③, DEPLOY-8). Fallback torch automatique si l'ONNX est indisponible ; GLiNER reste en torch (pas d'export ONNX dans gliner 0.2.12) |
| `CLOISON_ONNX_INT8` | detect | non | `1` | `0` = conserver l'export ONNX fp32 (référence précision) au lieu de la quantisation dynamique int8 |
| `CLOISON_DETECT_CONCURRENCY` *(DEPLOY-10)* | detect | non | `0` | `0` = illimité (défaut historique) ; `>0` = nombre maximal de pipelines `/detect` simultanés (protège le CPU partagé sous charge — dette secondaire §6bis). NB : l'inférence n'est PAS sérialisée par verrou (les verrous ne protègent que le chargement lazy) ; le goulot réel est la capacité CPU (ONNX int8 déployé, GPU en attente) |
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
| `CLOISON_DETECT_URL` *(B.1)* | ops | non | — | URL REST `POST /detect` du sidecar NER — **lu par le binaire** : les spans PERSON/LOC du sidecar sont fusionnés à la détection embarquée (validation core). Absent = détection embarquée seule. Dégradation gracieuse : sidecar indisponible → warn, jamais d'erreur |
| `CLOISON_DETECT_TIMEOUT_MS` *(B.1)* | ops | non | `2000` | timeout de la requête detect — au-delà, dégradation gracieuse (détection embarquée seule) |
| `CLOISON_CONTROL_URL` *(C)* | ops | non | absent | **voir §1** — activer après provisionnement du tenant + hash du jeton (`deploy/provision_control.sh`), sinon l'auth fail-closed renvoie 401 |

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
