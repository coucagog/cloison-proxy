# CLOISON — STACK-4 : Mode Audit (premier produit livrable)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Livrer le **premier produit commercialisable** : le mode Audit — observation seule. Le
proxy détecte et **compte** les PII sans rien masquer ni casser (audit de conformité),
produit des **reçus signés** (compteurs uniquement, jamais de texte), et génère des
**rapports k-anonymes** présentables à un régulateur.

## Périmètre

**Dans :** crate `cloison-audit` (reçus signés Ed25519, k-anonymat, rapports), intégration
observe-only dans le proxy (mode global `CLOISON_AUDIT_MODE` ou header par requête),
documentation de la séparation stricte corpus.

**Hors :** le plan de contrôle cloud (STACK-5), le NER (STACK-6).

## Décisions

1. **Observe-only** : le corps passe dans le `Detector` mais n'est jamais remplacé ; seuls
   `masked_by_type` (compteurs par type) sont incrémentés. Le client reçoit son texte intact.
2. **Reçu signé** : `{tenant_id, session_ref (BLAKE3 haché), ts, engine_version, policy_hash,
   agent_key_id, counters, sig_agent}`. Message signé = JSON canonique (sans espace, clés
   triées). **Jamais de texte** — invariants testés.
3. **K-anonymat à deux dimensions** : publiable ssi `requêtes >= k` ET chaque compteur
   `>= k` (k=5 défaut). Une requête × 6 emails → cellule non publiée (corrigé P0-2).
4. **Rapport signé** : `sig_report` Ed25519 sur `{period_start, period_end, total_requests,
   redacted}` — jamais les bruts. Le champ `aggregated` est `#[serde(skip_serializing)]` :
   le JSON servi au client ne contient que `redacted` (corrigé P0-1, P0-3).
5. **Séparation corpus stricte** : le flux d'audit n'alimente jamais le corpus (aucune API
   texte→persistance, reçus sans texte, invariant par construction). Sources corpus =
   publiques + synthétique + opt-in explicite.

## Ce qui a été construit

- `crates/cloison-audit/` : `receipt.rs` (Counters, Receipt, sign/verify, JSON canonique,
  session_ref haché), `k_anonymity.rs` (is_publishable 2D, aggregate, redact_below_k),
  `report.rs` (ConformanceReport, sign_report, verify_signature), `error.rs`.
- Proxy : `AuditEngine` (détecte+compte sans masquer), flux observe-only dans les 3 routes
  (chat, completions legacy, stream SSE count-only), header `X-Cloison-Audit-Receipt`,
  `GET /v1/audit/report` (404 hors mode audit).
- Clés Ed25519 : chargées depuis fichier (32 bruts/64 hex) ou générées 0600.
- Tests : 34 unitaires/integration audit + 5 e2e audit dans le proxy.

## Comment lancer / tester

```bash
cd cloison && source ~/.cargo/env
cargo test -p cloison-audit          # 34 tests (reçus, k-anonymat, rapport)
cargo test -p cloison-proxy --test e2e_audit   # mode audit e2e
# Mode audit au runtime : CLOISON_AUDIT_MODE=1 CLOISON_AUDIT_KEYS=<fichier> \
#   cargo run -p cloison-proxy ; GET /v1/audit/report?period=all
```

## Résultats

- **Tests** : audit 34/34, proxy 22/22 (11 STACK-3 préservés + 5 audit + 6 unit).
- **Clippy** : `-D warnings` → 0 erreur, 0 warning (audit + proxy + core).
- **Revue QA indépendante** : verdict GO conditionnel → **3 P0 identifiés et corrigés** :
  1. Rapport exposait les compteurs bruts (`aggregated`) → `skip_serializing` + tests.
  2. K-anonymat sans dimension requêtes → `is_publishable(request_count, counts)` + test
     « 1 requête × 6 emails → non publiable ».
  3. Rapport non signé → `sig_report` Ed25519 vérifiable (test tampering).
- **Preuve** : JSON du rapport inspecté — pas de `aggregated`, `sig_report` vérifiée.

## Invariants de sécurité vérifiés

1. **Zéro PII dans les reçus** : compteurs uniquement, `session_ref` haché, tests de
   non-fuite de texte.
2. **Preuve sans texte** : reçus signés vérifiables, jamais de contenu.
3. **K-anonymat réel** : seuils appliqués sur requêtes ET compteurs, bruts jamais servis.
4. **Corpus séparé** : aucune voie d'écriture du flux audit vers le corpus.
5. **Ne casse rien** : `audit_mode=false` → flux STACK-3 identique (tests e2e).

## Questions ouvertes / dette

- Journal des reçus **en mémoire** : perte au restart. Un stockage JSONL 0600 avec
  rétention est à ajouter (STACK-5 ou 7).
- `session_ref_hashed` = hash d'un `request_id` court : à renforcer (hash du tenant +
  session réels) en STACK-5.
- Le paramètre `period` de l'API est accepté mais pas encore filtrant (agrégation sur tout
  le journal) — à implémenter avec la persistance.

## Porte de sortie

- [x] Mode observe-only : texte non masqué, compteurs incrémentés.
- [x] Reçus signés Ed25519, jamais de texte.
- [x] Rapports k-anonymes signés, bruts jamais exposés.
- [x] Séparation corpus documentée et garantie par construction.
- [x] Tests + clippy verts, P0 QA corrigés.
- [x] **1er produit livrable** atteint.

## Prochaine étape

**STACK-5 — `cloison-control` + `cloison-ledger`/`cloison-verify`** : locataires, licences,
politiques (Postgres, 0 PII), signatures Ed25519, journal de transparence append-only
vérifiable (chaîne de hachage), vérificateur public. Le plan de contrôle aveugle.
