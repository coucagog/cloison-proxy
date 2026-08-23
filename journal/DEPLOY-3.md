# CLOISON — DEPLOY-3 : Surface publique du journal (journal.wonkom.ai)

> Journal de déploiement — étape C de la campagne (après DEPLOY-1 et DEPLOY-2).
> Conforme à la charte §8 (« API publique de transparence — journal.wonkom.ai :
> consultation + vérification d'inclusion, lecture seule, agrégée ») et au
> THREAT-MODEL §3.1 (exposition minimale : contrôle jamais public).

## Objectif

Exposer le **ledger de transparence** (compteurs k-anonymisés signés, jamais
de texte — invariant I9) en **lecture seule** et **vérifiable publiquement** :
`journal.wonkom.ai` sert le journal + la clé publique du contrôle + une page
dont le **WASM `cloison-verify`** valide la chaîne et les signatures **dans le
navigateur** (vérification décentralisée, rien ne quitte la page).

## Architecture (décisions)

1. **Conteneur `journal` dédié** (nginx alpine non-root, `read_only` + tmpfs,
   `cap_drop ALL`, port publié **uniquement sur 127.0.0.1** — joignable par
   Caddy seul, jamais directement par le public) : sert
   `/ledger.jsonl` + `/control_pubkey.hex` (volume `control-data` monté
   **en lecture seule**) + la page + le WASM.
2. **Clé de signature du contrôle STABLE** (`CLOISON_CONTROL_SIGNING_KEY` dans
   le `.env`, générée une fois) : le ledger doit rester vérifiable entre
   redémarrages (avant : clé éphémère → signatures invérifiables).
3. **Clé publique du contrôle écrite au boot** à côté du ledger
   (`/data/control_pubkey.hex`, écriture atomique tmp+rename) — code
   `cloison-control/src/main.rs` ; sans exposer l'API admin.
4. **Ledger en 0644** (`cloison-ledger` store) : le ledger de TRANSPARENCE est
   public par design (compteurs uniquement) ; le conteneur journal (nginx uid
   101) doit le lire sans courir sous l'uid du contrôle. Le journal d'AUDIT du
   proxy (JSONL 0600) reste strictement privé.
5. **WASM `cloison-verify`** : `crate-type = ["rlib","cdylib"]` (un rlib pur ne
   produit aucun `.wasm` sur wasm32) + build dans le Dockerfile du journal
   (wasm32-unknown-unknown, feature `wasm`, wasm-bindgen-cli épinglé 0.2.127) ;
   glue `--target web` → `cloison_verify.js` + `cloison_verify_bg.wasm`.
6. **Caddy** : bloc `journal.wonkom.ai → 127.0.0.1:8789` (mêmes émetteurs
   ACME, zéro log d'accès) ; le DNS pointait déjà vers 144.217.81.251.

## Actions réalisées

| # | Action | Résultat |
|---|---|---|
| C.1 | control : clé stable + écriture `control_pubkey.hex` (atomique) | ✅ |
| C.2 | ledger 0644 (transparence publique) | ✅ |
| C.3 | verify : `crate-type` cdylib → `.wasm` produit | ✅ |
| C.4 | `deploy/Dockerfile.journal` (build WASM + nginx) + `journal-html/` + `nginx-journal.conf` | ✅ |
| C.5 | compose : service `journal` (127.0.0.1:8789, volume RO) + Caddy | ✅ |
| C.6 | Déploiement + vérifications | ✅ |

## Résultats

- **`https://journal.wonkom.ai`** : page 200, TLS Let's Encrypt émis
  (CN=journal.wonkom.ai, validité 90 j, renouvellement auto).
- **`/ledger.jsonl`** : 200 — la genèse (seq 0) est servie ; le journal
  s'enrichira avec les reçus ingérés par le pipeline control (aujourd'hui :
  1 entrée, genèse — état honnête, aucun trafic client).
- **`/control_pubkey.hex`** : 200 (64 hex).
- **`/verify/cloison_verify.js` + `_bg.wasm`** : 200 (`application/wasm`).
- **Aucun crash** : 0 événement OOM (memwatch), mémoire 4,3 Go libres.
- **Sécurité** : contrôle strictement interne (THREAT-MODEL §3.1) ; le
  conteneur journal est lecture seule, non-root, sans capabilities, port
  bouclé 127.0.0.1 ; aucune PII (compteurs k-anonymes, invariant I9).

## Dette / suite

- **Alimenter le journal** : les entrées apparaissent via le pipeline
  control (contresignature des reçus). À terme, la page affichera des
  entrées réelles (tête seq > 0, preuves d'inclusion significatives).
- Page d'accueil racine (`wonkom.ai`) : non traitée (hors périmètre).
- `cloison-core` : le build WASM du core (rédaction navigateur `@cloison/core`)
  reste une distribution à part (même mécanique que verify si nécessaire).
- Fichiers temporaires de diagnostic supprimés ; le script de provisionnement
  des modèles NER (`download_models.py`) reste à déplacer dans `deploy/`.

## Porte de sortie (campagne A+B+C)

- [x] A — finitions (sonde cert J-14, memwatch service, constats).
- [x] B.1 — wiring edge→detect (preuve NER, e2e mock 10/10, réel 5/5).
- [x] B.2 — préchargement des modèles au boot.
- [x] C — surface publique du journal (lecture seule, vérifiable via WASM).
- [x] 0 OOM sur toute la campagne ; journaux DEPLOY-1/2/3 + commits poussés.
