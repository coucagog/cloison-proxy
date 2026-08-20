# CLOISON — Modèle de données

> Sources de vérité : `crates/cloison-ledger/src/{entry,ledger,store}.rs`,
> `crates/cloison-audit/src/{receipt,report,k_anonymity}.rs`,
> `crates/cloison-core/src/{token,policy,vault}.rs`,
> `crates/cloison-control/src/{model,store}.rs`,
> `services/cloison-detect/proto/detect.proto`.
> Règle d'or transversale : **aucune PII en dur** — seuls des hash, des
> compteurs et des signatures quittent le processus edge.

## 1. Sentinelles et jetons (`cloison-core`)

- **Jeton** : `TokenBody` = 16 octets = `mac_part (8)` ‖ `value_part (8)`,
  où `mac_part = HMAC-BLAKE3(mac_key, canonical_value ‖ kind_tag)[0..8]` et
  `value_part = BLAKE3(canonical_value)[0..8]`.
- **Clés de session** : `SessionKeys::derive(tenant_key, session_salt)` —
  HKDF-SHA256 (extract sur `session_salt` ‖ `tenant_key`, expand `mac`/`enc`).
- **Sentinelle** (format texte) : `⟦body_b32·TAG⟧` — `body_b32` = base32
  RFC 4648 minuscule sans padding (26 caractères pour 16 octets), `TAG` =
  2–4 lettres majuscules : `EM` (email), `PH` (PhoneSn), `CN` (CniSn),
  `CC` (carte), `IP`, `DT` (date), `GZ*` (gazetteer).
- **Registre d'émission** : périmètre de la requête en cours ; `restore`
  exige présence au registre **et** MAC valide (invariant I2).

## 2. Journal de transparence (`cloison-ledger`)

### 2.1 `LedgerEntry` (JSON sérialisable)

| Champ | Type | Rôle |
|---|---|---|
| `seq` | u64 | numéro séquentiel ; genèse = 0, entrées réelles à partir de 1 |
| `prev_hash` | [u8; 32] | `entry_hash` de l'entrée précédente ; genèse = `[0u8; 32]` |
| `entry_hash` | [u8; 32] | `SHA-256(header canonique)` |
| `payload_hash` | [u8; 32] | `SHA-256(JSON canonique compact du payload)` |
| `ts_unix` | u64 | horodatage UTC, non décroissant le long de la chaîne |
| `sig` | Vec<u8> | signature Ed25519 du contrôle sur `entry_hash` (64 o) ; vide pour la genèse |

Header binaire canonique (80 octets) :
`seq.to_le_bytes() ‖ prev_hash ‖ payload_hash ‖ ts_unix.to_le_bytes()`.

**Append** (`Ledger::append`) : rejette si `seq != len`, `prev_hash != head`,
`entry_hash` recomputé ≠ stocké, signature invalide. **Vérification**
(`verify_chain`) : genèse correcte, séquences consécutives, hash recomputés,
`ts_unix` non décroissant, signatures `verify_strict` (rejet de la
malléabilité). Toute altération d'un seul octet casse `entry_hash` **et** le
`prev_hash` de l'entrée suivante.

### 2.2 `LedgerPayload` (contenu engagé, jamais stocké en clair dans la chaîne)

| Champ | Type | Contenu |
|---|---|---|
| `schema_version` | u8 | 1 |
| `kind` | string | ex. `"conformance-period"` |
| `tenant_id` | string | identifiant opérateur non sensible |
| `period_start` / `period_end` | u64 | fenêtre de la période |
| `total_requests` | u64 | compteur agrégé |
| `counters` | BTreeMap<string, u64> | cellules **déjà k-anonymisées** (clés triées → JSON canonique) |
| `receipt_hashes` | Vec<[u8; 32]> | engagements sur les reçus STACK-4 (les reçus restent hors journal) |

`payload_hash = SHA-256(JSON canonique compact)` — un JSON ré-ordonné ne
correspond pas (inclusion échoue, comportement voulu).

### 2.3 Persistance

`Ledger` est un stockage **pur en mémoire** (genèse ensemencée à la
construction). Le binaire control (STACK-7) le persiste en **JSONL** dans
`CLOISON_LEDGER_FILE` (une `LedgerEntry` par ligne) ; relecture par
`Ledger::from_entries` + `verify_chain`.

## 3. Reçus d'audit (`cloison-audit`)

### 3.1 `Receipt` (JSON, version de schéma 1)

| Champ | Type | Contenu |
|---|---|---|
| `tenant_id` | string | identifiant opérateur non sensible |
| `session_ref_hashed` | string | `hex(SHA-256(tenant_id ‖ ":" ‖ session_ref))` — ne révèle ni la session ni sa clé |
| `ts_unix` | u64 | horodatage UTC |
| `engine_version` | string | version du moteur |
| `policy_hash` | string | `hex(SHA-256(JSON canonique de la Policy))` |
| `counters` | Counters | **entiers uniquement** (jamais de texte/span) |
| `sig_agent` | Vec<u8> | Ed25519 (64 o) sur `signing_bytes()` |

`Counters` : `masked_by_type: BTreeMap<String,u64>` (clés = `DetectorKind`
: `"Email"`, `"PhoneSn"`, `"CniSn"`…), `incomplete_restorations`,
`blocked_outputs`, `quasi_id_flags` (u64).

**Signature** : `signing_bytes()` = JSON canonique compact des champs hors
`sig_agent` (ordre de déclaration + clés triées) — déterministe entre
machines (I-A4). **Transport** : `base64url(canonical_json(receipt))` en
header `X-Cloison-Audit-Receipt` (réponses non-stream en mode audit).

### 3.2 Rapport de conformité (`ConformanceReport`, k-anonyme)

`GET /v1/audit/report?period=hourly|daily|weekly|all` — agrège le journal
du processus : une cellule n'est **publiée** que si
`requests ≥ k ∧ count ≥ k` ; un tenant que si `request_count ≥ k` ;
`session_ref` jamais présent (I-A6).

## 4. Plan de contrôle (`cloison-control`)

### 4.1 Modèle (`model.rs`)

- `Tenant { id, nom_public, statut, created_at }`
- `ApiToken { id, tenant_id, token_hash (hex SHA-256, jamais le clair),
  scopes, created_at, revoked_at?, rotated_at? }` — actif ssi non révoqué et
  non roté
- `Policy { tenant_id, json_policy, version, updated_at }` (version
  incrémentée à chaque publication)
- `License { tenant_id, plan, limites (LicenseLimites), expires_at?,
  created_at }` — une licence par tenant (upsert)

### 4.2 Store

- `InMemoryStore` (implémentation actuelle, `RwLock<HashMap>`).
- **Cible STACK-7 `PostgresStore`** (feature `pg`, sqlx) — schéma documenté
  dans `crates/cloison-control/src/store.rs` :
  `tenants`, `licenses`, `policies`, `tokens`, `ledger_entries`.
  Règles du contrat `Store` : `hash_token` fourni par le trait (le store ne
  voit **jamais** le clair) ; `validate_token` compare les digests en temps
  constant ; rotation = ancien marqué roté (plus aucun usage).

## 5. Contrat de détection (`cloison-detect`, proto `cloison.detect.v1`)

### 5.1 `DetectRequest`

| Champ | Type | Rôle |
|---|---|---|
| `text` | string | unité de texte UTF-8, offsets caractères |
| `locale` | string | BCP-47 : `"fr"`, `"fr-BF"`, `"wo"`, `"yo"`… |
| `policy` | Policy | types demandés, `min_score` (défaut 0.40), `thresholds` par type, `mode` (`balanced`/`high_precision`/`recall_only`), `enable_alias_expansion` (défaut true), `enable_quasiid_gauge` (défaut false), `models`, `quasiid_threshold` (défaut 0.50) |
| `session` | SessionContext | mentions établies (alias R1–R7) |
| `core_spans` | repeated Span | spans déjà établis par le core (fusion + jauge) |

### 5.2 `DetectResponse`

| Champ | Type | Rôle |
|---|---|---|
| `spans` | repeated Span | `{start, end, type, score}` — `type` ∈ `PERSON, LOC, ORG, DATE, AGE, ACT, ID` (enum `SpanType`) |
| `quasi_id` | QuasiIdReport? | `{score ∈ [0,1], flagged, signals[]}` (`"age"`, `"act"`, `"date"`, `"loc"`) — **signal uniquement, jamais de résolution** |

Contrat : `0 ≤ start < end ≤ len(text)` ; start/end ne coupent pas un point
de code UTF-8 ; le core **valide** l'alignement contre sa tokenisation.

### 5.3 Contexte de session (alias, règles R1–R7)

`SessionContext.mentions[]` : `{key (forme canonique, ex. "Marie Dupont"),
type (PERSON|LOC), locale, seen_count}`. L'expansion d'alias (`src/alias.py`)
résout `Momo → Mamadou`, `Ouaga → Ouagadougou`, titres (`M.`, `Mme`,
`Dr`…), formes dérivées (prénom + initiale, cap 8 formes, score ≤ 85 % du
canonique) ; les pronoms ne sont **jamais** traités comme des fuites.

### 5.4 Jauge de quasi-identifiants (`src/quasi_id.py`)

Fenêtre glissante (160 caractères, pas 40) : densité de catégories
(age+acte+date+lieu), bonus plafonné « plus de 4 mentions » (≤ 0.20),
score normalisé dans [0,1], `flagged = score ≥ seuil` (défaut 0.50 ;
1.0 désactive). Signale, ne résout pas.
