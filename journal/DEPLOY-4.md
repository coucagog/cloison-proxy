# CLOISON — DEPLOY-4 : Journal alimenté + normalisation rustfmt + reprise

> Journal de déploiement — traitement des deux points restants après
> DEPLOY-3, et préparation de la session suivante.

## Objectif

1. **Alimenter le journal de transparence** (il était à la genèse) via le
   **vrai pipeline** : edge (mode audit observe-only) → reçus signés →
   `POST /v1/control/ingest` → entrée contresignée dans le ledger → visible
   sur `journal.wonkom.ai`.
2. **Normaliser le formatage** (`cargo fmt`, formateur 1.97) : la dérive
   pré-existante (304 diffs sur des fichiers non touchés) rendait le job CI
   `fmt` impossible à satisfaire ; commit dédié.
3. Documenter pour la **prochaine session** (`journal/REPRISE-DEPLOIEMENT.md`).

## Décisions & actions

### 1. Journal alimenté (semis honnête, zéro PII)

- **Pipeline réel** : le edge relancé en mode audit (`CLOISON_AUDIT_MODE=1`)
  a généré sa clé d'agent (`/data/audit_key`, 32 octets) ; **3 requêtes
  synthétiques** (textes de test : Aminata Diop, Ibrahima Sarr, Fatou Ndiaye
  — aucune PII réelle) ont produit **3 reçus signés** (`/data/audit_ledger.jsonl`).
- **Clé agent → contrôle** : clé publique dérivée du seed
  (`f092bb56…a7c564`, `python3-cryptography`), passée au contrôle via
  `CLOISON_AGENT_VERIFY_KEY` (ajoutée au compose + `.env` + `.env.example` —
  elle n'était **pas** transmise au conteneur : l'ingest aurait échoué sur
  `sig_agent`).
- **Ingest** : `POST /v1/control/ingest` (réseau interne) avec les 3 reçus →
  `{"seq":1,"root_hash":"9dfcf152…"}` — **le ledger passe de 1 à 2 lignes**
  (genèse + entrée seq 1, chaîne liée via `prev_hash`).
- **Vérification** : `https://journal.wonkom.ai/ledger.jsonl` sert 2 lignes ;
  la page WASM valide la chaîne (clé de contrôle stable). **Le edge est
  remis en mode masquage** (`CLOISON_AUDIT_MODE=0`) — le produit redevient
  pseudonymisant.

### 2. Normalisation rustfmt

- `cargo fmt --all` (rustfmt 1.97) : **41 fichiers**, 1600 insertions /
  609 suppressions — style seul.
- **Piège rencontré** : le formateur 1.97 a corrompu la syntaxe d'un arm de
  `match` pré-existant (`config.rs` : virgule après commentaire) —
  `expected pattern, found ','`. Le motif (commentaire entre l'expression et
  la virgule de fin d'arm) est un piège du formateur récent ; corrigé en
  déplaçant le commentaire avant l'arm. **Leçon : après une normalisation
  rustfmt, `cargo fmt --check` ne suffit pas — un `cargo test`/`clippy` de
  contrôle est requis** (exécuté : vert).
- Résultat : `cargo fmt --all -- --check` **0 diff, 0 erreur** ; tests +
  clippy re-vérifiés (voir ci-dessous).

## Résultats

- `cargo fmt --check` : ✅ 0 diff (workspace complet, formateur 1.97).
- `cargo clippy --workspace --all-targets -- -D warnings` : ✅ 0 erreur.
- `cargo test --workspace --locked` : ✅ 0 échec (compte total consolidé dans
  le log de la session).
- Ledger public : 2 entrées (genèse + seq 1), vérifiable sur la page WASM.
- 0 événement OOM (memwatch) ; stack stable.

## Dette / suite

- **Ingest automatique** : aujourd'hui l'ingest est manuel (script). Le
  wiring « proxy → contrôle » (envoi automatique des reçus d'audit) reste à
  construire (dette STACK-4/5 : « le proxy ne consomme pas /v1/control/version »
  et l'ingest n'est pas automatisé).
- **CI fmt** : la normalisation aligne le dépôt sur rustfmt 1.97 ; si la CI
  utilise un rustfmt antérieur, l'épingler explicitement (dtolnay
  rust-toolchain avec version) pour éviter toute dérive future.
- Le script de provisionnement des modèles NER (`download_models.py`) reste à
  déplacer dans `deploy/` (il vit pour l'instant dans le home du serveur).
- `SERVER-SPECS.png` : abandonné (instruction MLS) — aucune action.

## Porte de sortie

- [x] Journal alimenté via le pipeline réel (seq 1, vérifiable publiquement).
- [x] Rustfmt normalisé (0 diff) + contrôle tests/clippy verts.
- [x] `CLOISON_AGENT_VERIFY_KEY` transmis au contrôle (compose + docs).
- [x] Journaux DEPLOY-1/2/3/4 + `REPRISE-DEPLOIEMENT.md` poussés.
