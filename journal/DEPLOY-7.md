# CLOISON — DEPLOY-7 : Publication open-core publique (dette ①)

> Journal de campagne — exécution de la dette ① de `journal/REPRISE-DEPLOIEMENT.md`
> §6 (validée pilote, acte irréversible acté) : publication des composants ouverts
> en dépôts publics `coucagog/cloison-*`. Session du 23 août 2026.

## Objectif

Rendre la promesse « nous ne lisons pas » **vérifiable par n'importe qui** (charte
§5.1 : l'open source est la condition de la promesse) : publier les 10 composants
ouverts (`cloison-core`, `cloison-proxy`, `cloison-audit`, `cloison-control`,
`cloison-ledger`, `cloison-verify`, `cloison-cli`, `cloison-wasm`,
`services/cloison-detect`, `bench/cloison-bench`) en dépôts publics, avec les
licences tranchées (proxy AGPL-3.0, reste Apache-2.0), et vérifier chaque sous-arbre.

**Gardes (REPRISE-DEPLOIEMENT §6) :** zéro PII / zéro secret par construction ;
`cloison-corpus` reste privé ; acte irréversible → validation pilote explicite actée.

## Décisions

1. **Source de vérité** : local == VPS == GitHub origin/main, tous à `4ed0c45`
   (vérifié) — les splits se font sur le VPS (`/home/debian/Cloison/cloison-open-core`,
   clone du dépôt de déploiement), conformément au principe « le repo vit sur l'hôte ».
2. **`git subtree split`** : 10 branches `pub/<composant>` (historique complet du
   préfixe — 69 commits replays) — l'exposition de l'historique est sûre car la
   porte de sécurité a été passée sur l'historique COMPLET (voir Résultats).
3. **Adaptation par sous-arbre** (un commit par repo) : `Cargo.toml` autonome
   (valeurs workspace inlinées), dépendances internes en **git deps épinglées**
   (`tag = "v0.1.0"`), texte de licence ajouté (`LICENSE` Apache-2.0 ; proxy :
   `LICENSE` + `LICENSE-AGPL-3.0` AGPL-3.0 officiel), README public dédié.
   `bench/cloison-bench` : `differential.py` nettoyé des chemins serveur absolus
   (env `CLOISON_CORE_BIN`/`CLOISON_DATASET`/`CLOISON_DIFF_OUT`, défauts relatifs),
   README aligné sur la grille v1.1 (5 conditions) + dépendance `cloison-detect`
   documentée.
4. **Ordre de publication = ordre de dépendance** : core, ledger → audit →
   verify, control, proxy → cli, wasm, detect, bench (les git deps taguées doivent
   exister avant que les dépendants ne les résolvent).
5. **Vérification en deux temps** : (a) pré-publication — workspace équivalent
   (8 crates adaptés, path deps restaurées) : `cargo test --workspace` + clippy +
   feature `pg` + fmt ; pytest detect (71) + bench (32) ; (b) post-publication —
   clone des dépôts publiés (tag v0.1.0) et `cargo test` sur chacun (git deps
   réelles résolues depuis GitHub) : on teste EXACTEMENT ce qui est publié.

## Portes de sécurité (avant tout push public)

- **0 secret réel** dans l'historique complet (`git log --all -p` scanné :
  ghp_/sk-or-/sk-proj-/BEGIN PRIVATE KEY/AKIA — zéro correspondance réelle, seuls
  des placeholders documentés `sk-or-v1-xxxxxxxx`, `mn_<32 hex>`).
- **0 PII réelle** : seuls des noms/CNI/téléphones SYNTHÉTIQUES (seed 42) et des
  numéros de test Luhn (`4242…`) ; la seule adresse mail de l'historique est
  l'auteur git du projet (`coucagog@gmail.com`, déjà public via le compte GitHub).
- **Aucun fichier sensible tracké** : pas de `.env`, de clé, de credential dans
  l'arbre ; `.gitignore` verrouille `.env*` (hors `.env.example`).

## Actions réalisées

| # | Action | Résultat |
|---|---|---|
| 1 | Vérification source de vérité (local/VPS/GitHub @ 4ed0c45) | ✅ |
| 2 | Portes de sécurité historiques (secrets/PII) | ✅ 0 / 0 |
| 3 | Fichiers d'adaptation préparés (10 Cargo.toml autonomes, 10 README, licences, differential.py) | ✅ |
| 4 | PREP VPS : clone + 10 `git subtree split` + 10 commits d'adaptation | ✅ |
| 5 | VERIFY pré-publication : workspace équivalent (cargo test/clippy/pg/fmt) + pytest | ✅ (voir Résultats) |
| 6 | PUBLISH : 10 dépôts publics + push main + tag v0.1.0 (ordre de dépendance) | ✅ |
| 7 | VERIFY2 post-publication : cargo test sur les dépôts publiés (git deps) | ✅ (voir Résultats) |
| 8 | Docs (OPEN-CORE.md §4, README) + journal + push | ✅ |

## Résultats

### Vérification pré-publication (workspace équivalent, path deps, rust:1.97)

- `cargo test --workspace` : ✅ tous verts (audit 17+17 · cli 1 · control 5+24 ·
  core 45+17 invariants · ledger 24+18 · proxy 11+11+7+5 · verify 19 · wasm 1).
- `cargo clippy --workspace --all-targets -- -D warnings` : ✅ 0 erreur.
- `cargo check -p cloison-control --features pg --locked` : ✅ (sqlx compile).
- `cargo fmt --all -- --check` : ✅ 0 diff.
- pytest `cloison-detect` (71, hors-ligne, image déployée) : ✅ **71/71**.
- pytest `cloison-bench` (32) : ✅ **32/32**.

### Publication

- 10 dépôts publics créés (HTTP 201) : `github.com/coucagog/cloison-{core,proxy,
  audit,control,ledger,verify,cli,wasm,detect,bench}` — vérifiés `public`,
  branche `main`, tag `v0.1.0` (push `pub/<composant> → main` + `refs/tags/v0.1.0`).
- Licences : proxy = **AGPL-3.0** (LICENSE + LICENSE-AGPL-3.0, texte officiel
  GNU) ; les 9 autres = Apache-2.0.
- **Correctif licence post-publication** : au premier passage, `LICENSE` du repo
  proxy contenait encore le texte Apache (copié du monorepo) — GitHub détectait
  Apache-2.0. Corrigé : `LICENSE` = texte AGPL-3.0 canonique (GNU), commit
  `b4f83f4` sur `main` + re-pointage du tag `v0.1.0` (release initiale, aucun
  consommateur externe ; changement = fichier de licence seul). Vérifié : la
  détection GitHub renvoie désormais **AGPL-3.0**.

### Vérification post-publication (dépôts publiés, git deps réelles)

- `cargo test` par repo publié cloné à la branche `main` (ordre de dépendance,
  **git deps résolues depuis GitHub** — fetch des repos publics vérifié) : ✅
  core 45+17 · ledger 24+18 · audit 17+17 · verify 19 · control 5+24 ·
  proxy 11+11+7+5 · cli 1 · wasm 1 — **tous verts**.
- `Cargo.lock` committé et poussé sur `main` pour les 8 repos Rust (les
  sous-arbres n'en portaient pas — le lock vit à la racine du monorepo ;
  doctrine épinglage DEPLOY-2 : reproductibilité) : core/ledger/audit/verify
  déjà à jour, control/proxy/cli/wasm committés pendant la campagne.

## Invariants de sécurité vérifiés

- **Zéro PII / zéro secret publié** : portes passées sur l'historique complet ;
  les dépôts publics ne contiennent que du code, des tests synthétiques et des
  métriques de benchmark (jamais de texte client).
- **`cloison-corpus` non publié** : aucun préfixe du corpus n'a été splité.
- **Licences conformes** à `docs/OPEN-CORE.md` §2 (proxy AGPL-3.0, reste Apache-2.0).

## Porte de sortie

- [x] 10 sous-arbres extraits (historique complet) et adaptés (autonomes, licences, README).
- [x] Vérifications pré- et post-publication vertes (cargo test/clippy/fmt/pg,
      pytest detect 71/71 + bench 32/32, puis tests des dépôts publiés avec git deps).
- [x] 10 dépôts publics créés, poussés (main + v0.1.0) dans l'ordre de dépendance.
- [x] `Cargo.lock` verrouillés et poussés sur les 8 repos Rust.
- [x] Docs à jour (OPEN-CORE.md §4, README) + journal + push.
- [ ] Reste à décision MLS : GPU (dette ②), priorisation ONNX (dette ③).

## Dette / suite

- `cloison-cli` et `cloison-wasm` publiés en squelettes (pas encore de logique
  produit) — cohérent avec le monorepo.
- Le README du repo principal peut évoluer vers un tableau de liens vers les
  dépôts publics (fait dans OPEN-CORE.md §4 ; README.md mis à jour).
- Prochaine publication (v0.2.0+) : re-split, re-vérifier, bumper les tags.
