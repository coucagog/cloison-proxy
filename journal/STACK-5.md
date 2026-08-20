# CLOISON — STACK-5 : Plan de contrôle aveugle (control + ledger + verify)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Construire le **plan de contrôle aveugle** : gestion des locataires/licences/politiques sans
aucune PII, journal de transparence append-only vérifiable, vérificateur public d'attestation.
Le cloud ne voit jamais de données personnelles — il ne manipule que des compteurs signés.

## Périmètre

**Dans :** `cloison-control` (API admin REST, store InMemory + trait), `cloison-ledger`
(chaîne de hachage + persistance append-only fichier), `cloison-verify` (vérificateur public,
WASM). Pipeline ingest : reçus audit → k-anonymat → payload redacté → contresignature → append.

**Hors :** le déploiement Postgres réel (trait Store prêt, impl Postgres = STACK-7), le NER
(STACK-6).

## Décisions

1. **Zéro PII par construction** : jetons `mn_` stockés **hachés** (SHA-256, domaine séparé),
   comparaison temps constant, le clair n'existe que dans la réponse d'émission
   (`TokenIssued::to_issued_json()`), Debug masqué, Serialize ne produit que le hash.
2. **Chaîne de hachage** : `entry_hash = SHA-256(header canonique 52 octets)` avec
   `prev_hash = entry_hash_{i-1}`, genèse `seq=0` non signée, signatures Ed25519 à partir de
   `seq=1`. `payload_hash = SHA-256(JSON canonique)`. Append-only garanti par séquenceur +
   refus de trous.
3. **Persistance** : `AppendOnlyFileLedger` (JSONL, mode 0600, jamais réécrit, flush+fsync
   par append, rechargé au boot). Trait `LedgerStore` + `MemLedger` pour les tests.
4. **Checkpoint anti-troncature** : `Checkpoint { seq, entry_hash, prev_cp_hash, ts, sig }`
   signé par le control ; `verify_chain_with_checkpoint` détecte une chaîne tronquée
   (`cp.seq > head` → TruncatedChain).
5. **IDOR bloqué** : rotate/revoke vérifient l'appartenance du jeton au tenant du chemin
   (store ET API), ordre de verrous global anti-interblocage.
6. **Révocation propagée** : `tokens_version` par tenant (incrémentée à chaque rotate/revoke),
   `GET /v1/control/version`, cache proxy par TokenView signé documenté.
7. **Rotation avec grâce** : l'ancien jeton reste valide `grace_until` (défaut 300 s,
   `CLOISON_ROTATION_GRACE_SECONDS`) puis expire.

## Ce qui a été construit

- `cloison-control/` : model (Tenant/ApiToken/License/Policy), trait Store + InMemoryStore,
  API axum (`/admin/tenants/*`, `/v1/control/ingest`, `/v1/control/root`,
  `/v1/control/version`, `/healthz`), contersign.rs (vérif sig_agent sur
  `receipt.signing_bytes()`, contresignature), token.rs (génération, hash, temps constant).
- `cloison-ledger/` : entry (SHA-256), ledger (append/verify/inclusion/root/genesis),
  checkpoint, store (MemLedger + AppendOnlyFileLedger), hexutil.
- `cloison-verify/` : verify_chain, prove_inclusion, verify_entry, find_inclusion,
  verify_chain_with_checkpoint, module wasm (feature `wasm`).

## Comment lancer / tester

```bash
cd cloison && source ~/.cargo/env
cargo test -p cloison-control -p cloison-ledger -p cloison-verify   # 88 tests
cargo clippy --workspace -- -D warnings
# Binaire control : cargo run -p cloison-control (CLOISON_CONTROL_PORT=8788)
# Vérificateur WASM : cargo check -p cloison-verify --target wasm32-unknown-unknown
```

## Résultats

- **Tests** : control 27, ledger 42, verify 19 = **88 verts** ; audit 34 intacts ;
  **199 tests au total dans le workspace**.
- **Clippy** : `-D warnings` → 0 erreur (control/ledger/verify/audit/proxy/core).
- **WASM** : `cloison-verify --target wasm32-unknown-unknown` compile.
- **Revue QA** : verdict **NO-GO initial** → tous les défauts corrigés :
  - P0-1 pipeline ingest control→ledger : construit et testé (e2e : reçu signé → k-anonymat
    → entrée contresignée → append).
  - P0-2 persistance : AppendOnlyFileLedger 0600, rechargé au boot, testé.
  - P0-3 contersign correct : vérification sur `signing_bytes()` (reçu réel passe, altéré
    refusé — testé).
  - P1 IDOR, troncature (checkpoint), format SHA-256, propagation version, grâce de
    rotation, fuite TokenIssued : tous corrigés et testés.

## Invariants de sécurité vérifiés

1. **Zéro PII** : jetons hachés, 0 texte, Debug/Serialize sans clair (testé anti-fuite).
2. **Journal infalsifiable** : toute modification/troncature détectée (tests tampering,
   suppression, inversion, troncature via checkpoint).
3. **Signatures valides** : Ed25519 verify_strict partout, contresignature correcte.
4. **Compteurs seuls** : le payload du ledger ne contient que des compteurs k-anonymes.
5. **Vérificateur public** : ne révèle rien de sensible (compteurs agrégés seulement).

## Questions ouvertes / dette

- `PostgresStore` : trait prêt, impl réelle en STACK-7 (déploiement).
- Le proxy ne consomme pas encore `GET /v1/control/version` (cache TokenView) : à brancher
  en STACK-7 (rodage réel).
- `GET /v1/audit/report` (STACK-4) et le ledger : le rapport peut s'appuyer sur le ledger
  comme source de vérité — à connecter.

## Porte de sortie

- [x] Plan de contrôle aveugle : tenants/licences/politiques/jetons hachés.
- [x] Journal append-only vérifiable (chaîne + signatures + checkpoint).
- [x] Vérificateur public buildable WASM.
- [x] Pipeline ingest audit→ledger complet et testé.
- [x] 88 tests verts, clippy 0, NO-GO QA résolu.

## Prochaine étape

**STACK-6 — `cloison-detect` (Python)** : NER transformer (GLiNER/AfroXLMR/SERENGETI),
expansion d'alias intra-session, jauge de quasi-identifiants, contrat gRPC
(`Detect(text, locale, policy) → spans[]`). Le rappel amélioré pour PERSON/LOC — le terrain
du fossé mesuré en STACK-1.
