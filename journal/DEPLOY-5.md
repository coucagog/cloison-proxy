# CLOISON — DEPLOY-5 : Wiring edge → contrôle (C), open-core, CI, PG réel, dettes

> Journal de déploiement — exécution de tous les chantiers ouverts de
> `journal/REPRISE-DEPLOIEMENT.md` §5, dans le respect de la charte
> `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (invariants §2). Session du 23 août 2026.

## Objectif

Solder les 6 chantiers de la reprise :

1. **Automatiser l'ingest** (LE chaînon manquant : audit → transparence) :
   le proxy envoie ses reçus d'audit au contrôle (`POST /v1/control/ingest`)
   et consomme `/v1/control/version` (long-poll rotation).
2. **Open-core** : composition + licences tranchées (AGPL passerelle),
   `docs/OPEN-CORE.md`, publication préparée (à décision MLS).
3. **CI** : rustfmt épinglé (1.97), image `journal` au matrix GHCR,
   feature `pg` vérifiée à la compilation.
4. **PostgresStore** : tests d'intégration PG réels exécutés sur la base du
   VPS (2/2) + migration vérifiée.
5. **Dettes produit** : `session_ref_hashed` sur une vraie session (jeton
   d'accès) ; calibration documentée.
6. **Hygiène** : `download_models.py` → `deploy/`, artefacts de diagnostic
   serveur supprimés.

## Décisions

1. **Auth edge par hash auprès du contrôle (wiring C)** : quand
   `CLOISON_CONTROL_URL` est posé, l'edge vérifie chaque jeton `mn_` par
   `POST /v1/control/verify` — seul `hex(SHA-256(domaine ‖ clair))` circule
   (le clair ne quitte jamais le bord ; le stockage du contrôle n'est que
   hash, invariant I2). Cache local TTL (défaut 300 s) + **fail-closed** :
   contrôle injoignable + cache froid → 401 (invariant I8). Sans URL (N0) :
   auth locale statique inchangée (charte §7.2 : résolu « localement ou via
   cloison-control »).
2. **Ingest automatique par lots** : intervalle `CLOISON_CONTROL_INGEST_INTERVAL_SECS`
   (défaut 60 s), lot ≤ 512 reçus, curseur **durable** (`<ledger>.ingested`,
   0600, écriture atomique) — un restart ne re-soumet pas les reçus déjà
   livrés (pas d'entrée dupliquée dans le journal de transparence). Échec →
   warn + retry (les reçus restent persistés dans le JSONL 0600).
3. **`session_ref_hashed` renforcé** : la référence de session des reçus est
   désormais le **hash du jeton d'accès** de la clé composite (session réelle,
   stable entre requêtes du même client) et non plus le `request_id`
   éphémère (dette STACK-4).
4. **Open-core / licences** : `cloison-proxy` passe en **AGPL-3.0**
   (anti-forks hébergés fermés, recommandation charte §5.1) ; les composants
   vérifiables restent Apache-2.0. La publication publique (création de
   dépôts publics) reste **à décision MLS** (acte irréversible ; procédure
   documentée dans `docs/OPEN-CORE.md`).
5. **CI** : rustfmt épinglé `dtolnay/rust-toolchain@1.97.0` (la normalisation
   DEPLOY-4 est faite avec 1.97 — le stable courant peut diverger) ; le
   matrix `images` inclut `journal` (l'image GHCR manquait) ; `test-rust`
   vérifie que la feature `pg` compile (`--locked`).
6. **Déploiement** : le tenant `default` + le **hash** du jeton edge existant
   sont provisionnés dans le contrôle (`deploy/provision_control.sh` — le
   clair est lu sur stdin, jamais persisté) — **aucune rupture client** : les
   clés composites existantes continuent de s'authentifier, désormais
   vérifiées par le contrôle.

## Ce qui a été construit

### Code — `cloison-proxy`
- `src/control.rs` (nouveau) : `ControlClient` (ingest/version/verify),
  `TokenVerifier` (cache TTL + purge sur montée de version + fail-closed),
  `flush_pending_audit` (lot + bornes de période), `token_hash` (même domaine
  que le contrôle), `MAX_INGEST_BATCH = 512`.
- `src/config.rs` : `ControlConfig` (`CLOISON_CONTROL_URL`,
  `CLOISON_CONTROL_INGEST_INTERVAL_SECS` 60, `CLOISON_CONTROL_POLL_INTERVAL_SECS` 30,
  `CLOISON_CONTROL_VERIFY_CACHE_TTL_SECS` 300, `CLOISON_TENANT_ID` default).
- `src/engine.rs` : `AuditEngine` — curseur d'ingest durable
  (`pending_receipts`, `mark_ingested`), fichier `<ledger>.ingested` 0600,
  rechargé au boot, borné à la longueur du journal.
- `src/handlers.rs` : `AppState.token_verifier` / `control` /
  `control_cfg` / `audit_k` ; `AppState::start_background_tasks()`
  (ingest + long-poll) ; `audit_build_and_record` prend le jeton d'accès
  comme `session_ref` (haché).
- `src/auth.rs` : vérification par le contrôle quand configuré
  (fail-closed), sinon comparaison statique (N0).
- `src/main.rs` : `start_background_tasks()` au boot.
- Tests `tests/e2e_control.rs` (nouveau) : auth par hash (200/401),
  fail-closed (panne + cache froid → 401 ; cache frais → 200), purge sur
  montée de version (long-poll), flush d'audit (lot, k, bornes, zéro PII,
  curseur avancé, pas de doublon), `session_ref` stable par jeton.

### Code — `cloison-control`
- `src/api.rs` : `POST /v1/control/verify` (`{tenant_id, token_hash}` →
  `{valid, version}` — tenant inconnu → `valid:false` sans erreur).
- `src/store.rs` : trait `Store::validate_token_hash` (+ InMemory).
- `src/postgres.rs` : `validate_token_hash` SQL (hash + tenant + état actif).
- Tests `tests/control.rs` : verify par hash (inconnu/révoqué/tenant
  inconnu/version), contrat de hash partagé avec le proxy.

### CI / deploy / docs
- `.github/workflows/ci.yml` : rustfmt 1.97.0 épinglé ; matrix `images` +
  `journal` ; `test-rust` + `cargo check -p cloison-control --features pg --locked`.
- `deploy/provision_control.sh` (nouveau) : tenant + hash du jeton
  (stdin, idempotent, anti-injection SQL).
- `deploy/download_models.py` (déplacé du home serveur → `deploy/`).
- `deploy/docker-compose.dev.yml` / `deploy/.env.example` /
  `deploy/helm/templates/deployment.yaml` : variables de wiring C.
- `docs/OPEN-CORE.md` (nouveau), `docs/CONFIG.md`, `docs/API.md`,
  `docs/ARCHITECTURE.md`, `docs/DEPLOY.md` (§11 wiring C, §12 tests PG),
  `docs/DATA-MODEL.md`, `README.md` à jour.
- `crates/cloison-proxy/Cargo.toml` : `license = "AGPL-3.0"` + `sha2`.

## Comment lancer / tester

```bash
# Tests Rust (workspace, y compris feature pg) :
docker run -d --name rustdev -v /home/debian/Cloison/cloison:/src rust:1.97-bookworm sleep infinity
docker exec rustdev sh -c 'export PATH=/usr/local/cargo/bin:$PATH; cd /src && cargo test --workspace'
docker exec rustdev sh -c 'cd /src && cargo clippy --workspace --all-targets -- -D warnings'
docker exec rustdev sh -c 'cd /src && cargo fmt --all -- --check'
docker exec rustdev sh -c 'cd /src && cargo check -p cloison-control --features pg --locked'

# Tests d'intégration PostgresStore (base réelle du VPS) :
docker network connect cloison-dev_cloison-internal rustdev
docker exec -e PG_PASS="$(grep -oP '(?<=^CLOISON_PG_PASSWORD=).*' .env | tr -d '"')" rustdev sh -c \
  'export PATH=/usr/local/cargo/bin:$PATH; cd /src && CLOISON_DATABASE_URL="postgres://cloison:${PG_PASS}@postgres:5432/cloison" \
   cargo test -p cloison-control --features pg --test postgres_store -- --ignored'

# Provisionnement du tenant + hash du jeton edge :
printf '%s' "$CLOISON_EXPECTED_ACCESS_TOKEN" | ./deploy/provision_control.sh default
```

## Résultats

### Gates CI (sur le serveur, conteneur rust:1.97)
- `cargo test --workspace` : **tous verts** (28 binaires, 0 échec) — dont
  les nouveaux tests e2e_control et control::api_verify_token.
- `cargo clippy --workspace --all-targets -- -D warnings` : **0 erreur**.
- `cargo fmt --all -- --check` : **0 diff** (rustfmt 1.97).
- `cargo check -p cloison-control --features pg --locked` : OK.

### PostgresStore sur le PG réel du VPS (dette STACK-8 réglée)
- `tenant_token_license_policy_roundtrip` : **ok** (roundtrip tenant/jeton
  haché/politique/licence, le clair jamais persisté).
- `rotate_and_revoke_with_idor` : **ok** (IDOR cross-tenant, grâce, version).
- Migration `001_init.sql` : re-appliquée par `PostgresStore::connect` au run
  (idempotente) ; 4 tables présentes.
- Fixtures de test (`t1/t2/t3`) supprimées après le run (hygiène prod).

### Déploiement (VPS 144.217.81.251) — VÉRIFIÉ
- **Rebuild** edge + control (buildkit cache), 5 conteneurs up, 0 OOM (memwatch).
- **Auth via contrôle prouvée** : `https://api.wonkom.ai/v1/models` avec le
  jeton réel → **200** (vérifié par `POST /v1/control/verify`) ; jeton inconnu
  → **401** (fail-closed) ; sans auth → 401.
- **E2E mock anti-pass-through : SUCCÈS** contre l'edge déployé (auth
  contrôle active) : sentinelles ⟦ reçues par le faux LLM, PII absente amont,
  PII restaurée côté client, zéro jeton résiduel.
- **Ingest automatique prouvé (LE chaînon manquant)** : mode audit
  (`CLOISON_AUDIT_MODE=1`) + 3 requêtes PII synthétiques → au tick suivant
  (60 s) : `reçus d'audit ingérés au contrôle receipts=3 seq=2` (log edge),
  `ledger entry appended tenant_id=default seq=2 k=5` (log control), et le
  **ledger public passe de 2 à 3 lignes** (`journal.wonkom.ai/ledger.jsonl`).
  Retour en mode masquage (`AUDIT_MODE=0`) vérifié.
- Provisionnement `default` + hash du jeton edge : fait (le clair jamais
  affiché ni persisté) ; fixtures de test PG supprimées.

### CI GitHub (push `05fcb15` — après correctifs)
- `fmt` : ✅ (rustfmt 1.97 épinglé).
- `clippy` : ✅ (toolchain 1.97 épinglée + lint `hexutil` corrigé).
- `test-rust` : ✅ (+ check feature `pg`).
- `test-detect` : ✅ (index CPU PyTorch pour `torch==2.5.1+cpu`).
- `images` / `bench` / `e2e-llm` : à confirmer sur le run suivant.

## Invariants de sécurité vérifiés

- **I2 (coffre au bord / hash)** : le clair `mn_` ne quitte jamais l'edge —
  seul le digest SHA-256 (domaine `cloison-mn-token-v1:`) circule vers le
  contrôle (testé : hash ≠ clair, stockage hash-only).
- **I8 (fail-loud)** : contrôle injoignable + cache froid → 401 (testé) ;
  échec d'ingest → warn + retry, jamais de perte (reçus persistés 0600).
- **I9 (preuve sans texte)** : le lot d'ingest ne contient que des reçus
  (compteurs + hash), aucun texte client (testé : le corps transmis ne
  contient pas la PII de test).
- **Zéro PII / zéro secret** : aucun secret affiché dans cette campagne ; le
  provisionnement lit le jeton sur stdin et ne persiste que le hash.
- **Append-only** : le curseur d'ingest ne supprime jamais de reçu du JSONL ;
  il avance seul (aucune entrée dupliquée après restart).

## Porte de sortie

- [x] Chantier 1 (ingest auto + long-poll rotation) : code + tests + docs +
      **preuve de bout en bout sur le VPS** (seq 2 visible publiquement).
- [x] Chantier 2 (open-core) : composition, licences, docs, procédure prête.
- [x] Chantier 3 (CI) : rustfmt épinglé, journal au GHCR, feature pg vérifiée,
      **CI réparée** (échecs clippy/test-detect pré-existants résolus).
- [x] Chantier 4 (PostgresStore réel) : 2/2 verts sur la base du VPS.
- [x] Chantier 5 (session_ref + calibration) : session réelle implémentée ;
      calibration documentée (seuils GO en prod, `measure_clusters.py`).
- [x] Chantier 6 (hygiène) : `download_models.py` → `deploy/`, artefacts
      `/tmp` supprimés (audit_key, seed_resp_*.json, ancien bundle).
- [x] Vérification de bout en bout sur le VPS : auth contrôle (200/401),
      e2e mock SUCCÈS, ingest automatique → ledger public (2 → 3 lignes),
      retour masquage vérifié.

## Dette / suite

- Publication publique open-core : **décision MLS requise** (docs/OPEN-CORE.md §4).
- Latence CPU detect (2-6 s/doc) : GPU conseillé en prod (inchangé).
- Calibration fine des seuils en prod : procédure documentée, à exécuter
  avec du trafic réel (`measure_clusters.py`).
- L'ingest ne consomme pas encore les reçus en mode masquage
  (`CLOISON_AUDIT_MODE=0` : aucun reçu généré — par design ; l'ingest
  automatique s'applique aux fenêtres d'audit observe-only).
