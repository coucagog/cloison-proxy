# CLOISON — API

> Sources de vérité : `crates/cloison-proxy/src/{routes,handlers,auth,errors}.rs`
> (edge), `crates/cloison-control/src/api.rs` (control),
> `services/cloison-detect/src/api.py` + `proto/detect.proto` (detect),
> `crates/cloison-verify/src/{lib,wasm}.rs` (vérification).

## 1. API edge (passerelle OpenAI-compatible, port 8787)

### 1.1 Authentification — clé composite

```
Authorization: Bearer mn_<jeton_acces>.<cle_amont>
```

- Découpage sur le **premier point** (`splitn(2, '.')`) : la clé amont peut
  contenir des points, ils sont conservés.
- Le jeton doit commencer par `mn_` (non vide au-delà) ; la clé amont non vide.
- Si `CLOISON_EXPECTED_ACCESS_TOKEN` est configuré : comparaison à temps
  constant (subtle). Échec → `401 invalid_api_key`, **aucun** appel amont.
- La clé amont n'est réinjectée que dans le header de la requête amont
  (jamais en URL, jamais en log — invariant I1).

### 1.2 `POST /v1/chat/completions`

Corps OpenAI-compatible. Comportement selon `stream` :

| Mode | Traitement | Réponse |
|---|---|---|
| `stream: false` | corps tokenisé → amont → restauration JSON | JSON OpenAI (`choices[0].message.content`) |
| `stream: true` | SSE buffer-and-scan ; restauration par jeton (sentinelles découpées sur les chunks reconstituées) ; keep-alive | `text/event-stream`, clôture `data: [DONE]` ; erreur en cours de flux → `data: {"error": …}` puis `[DONE]` |

Champs transformés (aller/retour) : `messages[].content`, `messages[].tool_calls[].function.arguments`, `prompt`. Champs inconnus : transmis intacts (I6). Sentinelle non résoluble → marqueur neutre `[REDACTED]` + compteur `unresolved_tokens` (fail-loud, I3).

### 1.3 `POST /v1/completions` (legacy)

Non-stream uniquement — `stream: true` → `400 invalid_request_error`
(implémentation SSE legacy explicitement hors périmètre, erreur nette).

### 1.4 `GET /v1/models`

Pass-through amont (aucune tokenisation) après auth.

### 1.5 `GET /v1/audit/report?period=hourly|daily|weekly|all`

**Mode audit uniquement** (`CLOISON_AUDIT_MODE=1`) — sinon `404 not_found`.
Retourne le rapport de conformité k-anonyme (`ConformanceReport`) du journal
du processus. `period` hors `all|hourly|daily|weekly` → `400`.

### 1.6 Reçus d'audit

En mode audit, chaque réponse **non-stream** porte le header
`X-Cloison-Audit-Receipt` : `base64url(canonical_json(Receipt))` — compteurs
entiers, signature Ed25519 (vérifiable par `cloison-verify`, cf. §4). En
stream, le reçu est accumulé à la clôture (pas de header).

### 1.7 Codes d'erreur normalisés (edge)

Shape : `{"error": {"message", "type", "code"}}` — le `message` ne contient
**jamais** de secret.

| Catégorie | HTTP | `type` | `code` |
|---|---|---|---|
| Auth | 401 | `authentication_error` | `invalid_api_key` |
| Accès | 403 | `permission_error` | `permission_denied` |
| Corps invalide | 400 | `invalid_request_error` | `invalid_request_error` |
| Corps trop volumineux | 413 | `invalid_request_error` | `request_too_large` |
| Ressource absente | 404 | `invalid_request_error` | `not_found` |
| Quota / débit | 429 | `rate_limit_error` | `rate_limit_exceeded` |
| Fournisseur | 502 | `server_error` | `upstream_error` |
| Timeout fournisseur | 504 | `server_error` | `upstream_timeout` |
| Interne | 500 | `server_error` | `internal_error` |

## 2. API control (plan de contrôle aveugle, port 8788)

Réseau interne uniquement. Aucun texte client ne transite ; le clair `mn_`
n'apparaît qu'une fois dans la réponse d'émission. Erreurs :
`{"error": "<message public>"}` — jamais de détail interne.

| Méthode | Route | Corps | Réponse |
|---|---|---|---|
| POST | `/admin/tenants` | `{id, nom_public, plan}` | `Tenant` (crée le tenant + une licence) |
| GET | `/admin/tenants/{id}` | — | `Tenant` ou 404 |
| POST | `/admin/tenants/{id}/tokens` | `{scopes?}` | `TokenIssued {id, token (clair, une seule fois), expires_at}` — seul le hash est stocké |
| POST | `/admin/tenants/{id}/rotate` | `{token_id}` | `TokenIssued` (l'ancien passe `rotated_at`, plus aucun usage) |
| DELETE | `/admin/tenants/{id}/tokens/{token_id}` | — | 204 (révocation immédiate) |
| PUT | `/admin/tenants/{id}/policy` | `{json_policy}` | `Policy {tenant_id, json_policy, version (incrémentée), updated_at}` |
| POST | `/admin/tenants/{id}/licenses` | `{plan, expires_at?}` | `License` (upsert, une par tenant) |
| GET | `/healthz` | — | `{"status":"ok","service":"cloison-control"}` |

Plans (`Plan`) et statuts : `Plan` (ex. `standard`/`pro` — défini dans
`model.rs`) ; `TenantStatut::Actif`. Contresignature des reçus d'audit :
`crates/cloison-control/src/contersign.rs` (validation de la chaîne Ed25519
du bord avant engagement dans le ledger).

## 3. API detect (sidecar NER, REST 8080 / gRPC 50051)

### 3.1 REST (FastAPI, repli)

| Méthode | Route | Corps | Réponse |
|---|---|---|---|
| POST | `/detect` | `DetectRequest` (JSON, `extra="forbid"`) | `{spans: [{start, end, type, score}], quasi_id?}` |
| GET | `/healthz` | — | `{status, models_loaded, models_pending}` |
| GET | `/version` | — | `{name: "cloison-detect", version, proto: "cloison.detect.v1"}` |
| GET | `/models` | — | `{models: {<nom>: {available, loaded, …}}}` |

`DetectRequest` : `{text, locale?, policy?, session?, core_spans?}` —
`policy.types` vide = tous ; `policy.models` demandé mais indisponible →
`503 {"error": {"code": "FAILED_PRECONDITION", …}}` ; offsets invalides →
`400 INVALID_ARGUMENT`. Erreurs : `{"error": {"code", "message"}}` — jamais
le texte d'entrée. Docs auto (openapi/redoc) désactivées.

### 3.2 gRPC (`cloison.detect.v1.DetectService`, nominal)

```proto
service DetectService { rpc Detect(DetectRequest) returns (DetectResponse); }
```

Messages : `Policy`, `SessionContext{mentions[]}`, `Mention{key,type,locale,seen_count}`,
`DetectRequest{text, locale, policy, session, core_spans[]}`,
`DetectResponse{spans[], quasi_id?}`, `Span{start,end,type,score}`,
`QuasiIdReport{score, flagged, signals[]}` (détails : `proto/detect.proto`).
Code généré : `python -m grpc_tools.protoc -I proto --python_out=src/gen
--grpc_python_out=src/gen proto/detect.proto` (fait au build de l'image ;
sans lui, gRPC est désactivé et le REST reste servi).

## 4. API de vérification (`cloison-verify`)

### 4.1 Rust (stateless, aucun secret)

| Fonction | Signature | Rôle |
|---|---|---|
| `verify_chain` | `(entries: &[LedgerEntry], control_key: &VerifyingKey) -> Result<(), VerifyError>` | revalide genèse, seq, prev_hash, entry_hash, ts, signatures |
| `verify_chain_v` | idem → `ChainVerdict {ok, entries_checked, head_seq, head_entry_hash, failure}` | verdict structuré |
| `verify_entry` | `(entry, prev_hash, control_key) -> Result<(), VerifyError>` | vérification incrémentale |
| `prove_inclusion` | `(entries, payload_hash) -> bool` | preuve d'inclusion par hash |
| `find_inclusion` | → `Option<InclusionProof {target_seq, target_payload_hash, prefix_hashes, head_seq, head_entry_hash}>` | preuve structurée |

`VerifyError` : `EmptyChain`, `GenesisMismatch`, `SeqGap`,
`PrevHashMismatch`, `EntryHashMismatch`, `BadSignature`,
`TimestampRegressed` — messages statiques, sans contenu utilisateur.

### 4.2 Exports WASM (`--features wasm`, cible `wasm32-unknown-unknown`)

| Export | Entrée | Sortie |
|---|---|---|
| `verify_chain_bytes(entries_json, control_key_hex)` | JSON des entrées + clé publique hex | `{"ok":true}` ou `{"ok":false,"error":"…"}` |
| `prove_inclusion_bytes(entries_json, payload_hash_hex)` | idem + hash hex | `true`/`false` |

Build : `cargo build -p cloison-verify --target wasm32-unknown-unknown
--features wasm` (+ `wasm-bindgen`). Le noyau est indépendant de
wasm-bindgen et reste testable nativement.
