# CLOISON — Architecture

> STACK-7 · Dernière mise à jour : build & packaging final.
> Document de vérité : le code (`crates/*`, `services/cloison-detect`). Toute
> divergence signalée ici est un bug de documentation.

## 1. Vue d'ensemble

CLOISON est un **proxy de confidentialité PII** placé entre une interface IA
et un fournisseur LLM (OpenRouter, DeepSeek…). À l'aller, le texte est
**tokenisé** (PII → sentinelles) ; au retour, les sentinelles émises par la
requête en cours sont **restaurées**. Le fournisseur ne voit jamais la PII ;
le client reçoit toujours son texte d'origine.

```
  Client IA ──► edge (cloison-proxy, :8787) ──► LLM réel (OpenRouter/DeepSeek)
                │  tokenisation/restauration PII
                │  mode audit observe-only (reçus Ed25519, rapport k-anonyme)
                │
                ├──► detect (cloison-detect, :8080 REST / :50051 gRPC)
                │        sidecar NER stateless (rappel PERSON/LOC, STACK-6)
                │
                └──► control (cloison-control, :8788) — plan de contrôle aveugle
                         tenants, jetons mn_* (hash seul), politiques, licences
                         journal append-only cloison-ledger (CLOISON_LEDGER_FILE)
                         vérification publique hors-ligne : cloison-verify (WASM)
```

## 2. Composants (crates)

| Crate | Rôle | Binaire ? | Port |
|---|---|---|---|
| `cloison-core` (STACK-2) | Tokenisation déterministe portable : détection (regex, gazetteers Aho-Corasick, Luhn), jetons HMAC-BLAKE3 + sentinelles `⟦b32·TAG⟧`, registre d'émission par requête, vault chiffré (redb + AES-256-GCM), généralisation. Buildable WASM (`src/wasm.rs`). | bibliothèque | — |
| `cloison-proxy` (STACK-3) | Passerelle OpenAI-compatible : `POST /v1/chat/completions` (stream/non-stream), `POST /v1/completions` (legacy, non-stream), `GET /v1/models`, `GET /v1/audit/report` (mode audit). Auth par clé composite `Bearer mn_<jeton>.<cle_amont>`. | **oui** (`src/main.rs`) | 8787 |
| `cloison-audit` (STACK-4) | Mode audit observe-only : reçus signés Ed25519 (`Receipt`, compteurs entiers uniquement), k-anonymat, rapport de conformité `ConformanceReport`. | bibliothèque | — |
| `cloison-control` (STACK-5) | Plan de contrôle aveugle : tenants, licences, politiques, jetons `mn_*` (seul le hash est stocké). API admin REST axum (`/admin/*`, `/healthz`) + pipeline ingest → ledger (contresignature). Persistance : `Store` trait — `InMemoryStore` aujourd'hui, `PostgresStore` en dette ouverte. | **oui** (`src/main.rs`) | 8788 |
| `cloison-ledger` (STACK-5) | Journal de transparence append-only vérifiable : chaîne de hachage (`entry_hash = SHA-256(header 80 o)`), signatures Ed25519 du contrôle, payload = compteurs k-anonymisés + hash de reçus (jamais de texte). | bibliothèque | — |
| `cloison-verify` (STACK-5) | Vérificateur public stateless : `verify_chain`, `prove_inclusion`, `InclusionProof`. Exports WASM (`verify_chain_bytes`, `prove_inclusion_bytes`). | bibliothèque (feature `wasm`) | — |
| `cloison-detect` (STACK-6, Python) | Sidecar NER stateless : Presidio (oracle FR, regex CNI, gazetteers) + GLiNER zéro-shot (lazy) + fusion pondérée + alias intra-session (R1–R7) + jauge quasi-id. Détecte uniquement — ne tokenise ni ne persiste rien. | **oui** (`python -m src.main`) | REST 8080, gRPC 50051 |

## 3. Flux de bout en bout (edge, mode masquage)

1. **Auth** : le client présente `Authorization: Bearer mn_<access_token>.<upstream_key>`
   (`crates/cloison-proxy/src/auth.rs`). Découpage sur le **premier point** ;
   validation à temps constant si `CLOISON_EXPECTED_ACCESS_TOKEN` est configuré ;
   la clé amont ne sort qu'en header de la requête amont (jamais en log/URL).
2. **Aller** : `POST /v1/chat/completions` → le corps complet est tokenisé
   (`cloison-core::engine`, registre d'émission = périmètre de la requête) :
   `content`, `tool_calls[].function.arguments`, `prompt` sont transformés ;
   les champs inconnus passent intacts (I6). PII → sentinelles `⟦b32·TAG⟧`.
3. **Amont** : le corps tokenisé est envoyé au fournisseur
   (`CLOISON_UPSTREAM_BASE_URL` + chemins configurables).
4. **Retour non-stream** : la réponse JSON est restaurée — seules les
   sentinelles émises par cette requête sont résolues (MAC vérifié). Une
   sentinelle non résoluble → marqueur neutre `[REDACTED]` + compteur (fail-loud).
5. **Retour stream** : SSE buffer-and-scan, restauration par jeton, sentinelles
   découpées sur les chunks reconstituées, keep-alive, clôture `[DONE]`,
   erreur en cours de flux → `data: {"error":…}` puis `[DONE]`.

## 4. Mode audit (observe-only, STACK-4)

`CLOISON_AUDIT_MODE=1` : le proxy **ne masque pas**, il compte. Corps aller et
réponse amont sont transmis à l'identique ; chaque requête produit un
**reçu signé** (`Receipt`, compteurs entiers uniquement) posé en header
`X-Cloison-Audit-Receipt` (non-stream) et accumulé dans le journal du
processus. `GET /v1/audit/report?period=hourly|daily|weekly|all` sert un
rapport de conformité **k-anonyme** (`cloison-audit::report`).

## 5. Plan de contrôle (STACK-5)

`cloison-control` sert l'API admin `/admin/*` (tenants, jetons, rotation,
révocation, politiques, licences) et `/healthz`. Aucun texte client ne
transite : le clair `mn_` n'apparaît qu'une fois dans la réponse d'émission.
Le journal `cloison-ledger` est append-only et vérifiable par
`cloison-verify` (chaîne + signatures). Le binaire serveur (`src/main.rs`,
posant le routeur sur :8788 et persistant `CLOISON_LEDGER_FILE`) est committé
(STACK-7) — cf. `docs/DEPLOY.md` §1.2.

## 6. Sidecar detect (STACK-6)

Stateless : `Detect(text, locale, policy, session, core_spans) -> spans[]`.
Transport nominal gRPC (`proto/detect.proto`), repli REST (`POST /detect`).
Le core Rust valide les spans contre sa propre tokenisation ; le sidecar ne
persiste rien et dégrade gracieusement si un modèle est absent (jamais de
crash). Wire edge→detect (`CLOISON_DETECT_URL`) : **cible STACK-7**, non
encore lu par le binaire.

## 7. Déploiement (STACK-7)

- **Images** : `deploy/Dockerfile.proxy` (edge), `deploy/Dockerfile.control`,
  `deploy/Dockerfile.detect` — multi-stage, runtime **distroless non-root**
  (uid 65532 ; 10001 pour detect), read-only + tmpfs. Voir `docs/DEPLOY.md`.
- **Compose dev** : `deploy/docker-compose.dev.yml` (hôte wonkom.ai).
- **Kubernetes** : charte Helm `deploy/helm/` (edge/control/detect, une
  charte, le rôle est une valeur).
- **CI** : `.github/workflows/ci.yml` — fmt/clippy/test, pytest, bench,
  build + SBOM (syft) + scans (grype/trivy) + cosign.
- **Distribution WASM** : `cloison-core` et `cloison-verify` buildables
  `wasm32-unknown-unknown` (features `wasm`) → rédaction PII dans le
  navigateur (`@cloison/core`) et vérification de reçus/chaîne côté client
  (`@cloison/verify`).

## 8. Références

- Modèle de données : `docs/DATA-MODEL.md`
- API complète : `docs/API.md`
- Configuration : `docs/CONFIG.md`
- Menaces : `docs/THREAT-MODEL.md` · Sécurité : `docs/SECURITY.md`
