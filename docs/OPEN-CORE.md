# CLOISON — Open-core : composition, licences, publication

> Décision consignée dans `journal/DEPLOY-5.md` (chantier 2). Conforme à la
> charte §5.1 (« publier `cloison-core`, `cloison-proxy`, `cloison-ledger`,
> `cloison-verify` — l'open source est la **condition** de la promesse
> « nous ne lisons pas » ») et §10 (corpus **privé**).

## 1. Pourquoi l'open-core

La promesse produit « vos données personnelles n'atteignent jamais le modèle en
clair, et nous ne les voyons pas » n'est crédible que si **l'architecture est
auditable par n'importe qui** : un client ne fait pas confiance à un contrat,
il vérifie le code. La charte §5.1 en fait la condition de la promesse.

## 2. Composition (décidée)

| Composant | Crate | Licence | Statut |
|---|---|---|---|
| Moteur (détection déterministe, jetons, coffre) | `cloison-core` | Apache-2.0 | **public** (à publier) |
| Passerelle OpenAI (le produit visible) | `cloison-proxy` | **AGPL-3.0** | **public** (à publier) |
| Journal de transparence | `cloison-ledger` | Apache-2.0 | **public** (à publier) |
| Vérificateur public (WASM) | `cloison-verify` | Apache-2.0 | **public** (à publier) |
| Mode audit / reçus signés | `cloison-audit` | Apache-2.0 | **public** (à publier) |
| Plan de contrôle aveugle | `cloison-control` | Apache-2.0 | **public** (à publier) |
| Outillage CLI | `cloison-cli` | Apache-2.0 | **public** (à publier) |
| Wrapper WASM navigateur | `cloison-wasm` | Apache-2.0 | **public** (à publier) |
| Sidecar NER lourd | `services/cloison-detect` | Apache-2.0 | **public** (à publier) |
| Harnais de benchmark + scoring | `bench/cloison-bench` | Apache-2.0 | **public** (méthodologie publique, charte §5.1) |
| **Corpus** (gazetteers étendus, specs CNI, tables de fréquence, jeux d'éval, catalogue des non-détections) | `cloison-corpus` (dépôt séparé) | **PROPRIÉTAIRE** | **privé** (jamais publié) |

### Pourquoi AGPL-3.0 pour la passerelle uniquement

Le proxy est le **seul** composant dont un fork hébergé pourrait « profiter »
de l'effort CLOISON sans contribuer (charte §5.1 : « empêcher les forks
hébergés fermés »). L'AGPL-3.0 impose la publication du code aux utilisateurs
en réseau (compatible SaaS) — un fork qui héberge la passerelle doit rendre
son code disponible. Les composants vérifiables restent Apache-2.0 (permissif,
réutilisation maximale — le code n'est pas le produit, la confiance l'est).

Décision **réversible avant la première publication publique** (le dépôt est
privé, aucun contributeur externe) : elle est consignée au journal pour
validation MLS.

## 3. Ce qui ne sera JAMAIS publié

- **`cloison-corpus`** : gazetteers détaillés, spécifications CNI, tables de
  fréquence de noms, générateurs synthétiques, jeux d'évaluation et le
  **catalogue des non-détections** (charte §10 — « PROPRIÉTAIRE »).
- **Aucune PII, aucun secret** : les invariants du dépôt (0 PII, 0 secret)
  s'appliquent par construction — une publication ne peut pas emporter de
  donnée client (vérifié avant tout push public).

> NB : `cloison-core` embarque des listes statiques de noms/toponymes
> sénégalais synthétiques/publics (`detection.rs`) nécessaires au produit. Ce
> sont des listes **embarquées de code** (publiques), distinctes du corpus
> privé (jeux d'évaluation, tables de fréquence, catalogue des non-détections).

## 4. Publication (EXÉCUTÉE — DEPLOY-7, 23 août 2026)

> ~~à décision MLS — non exécutée~~ → **EXÉCUTÉE** : dette ① de
> `journal/REPRISE-DEPLOIEMENT.md` §6 validée par le pilote pour la session,
> campagne journalisée dans `journal/DEPLOY-7.md`.

Les 10 composants ouverts sont publiés en dépôts **publics** `coucagog/cloison-*`
(branche `main` + tag `v0.1.0`), extraits par `git subtree split` depuis la source
de vérité (commit `4ed0c45`) :

| Composant | Dépôt public | Licence |
|---|---|---|
| Moteur | [coucagog/cloison-core](https://github.com/coucagog/cloison-core) | Apache-2.0 |
| Passerelle | [coucagog/cloison-proxy](https://github.com/coucagog/cloison-proxy) | **AGPL-3.0** |
| Journal | [coucagog/cloison-ledger](https://github.com/coucagog/cloison-ledger) | Apache-2.0 |
| Vérificateur | [coucagog/cloison-verify](https://github.com/coucagog/cloison-verify) | Apache-2.0 |
| Mode audit | [coucagog/cloison-audit](https://github.com/coucagog/cloison-audit) | Apache-2.0 |
| Plan de contrôle | [coucagog/cloison-control](https://github.com/coucagog/cloison-control) | Apache-2.0 |
| Outillage CLI | [coucagog/cloison-cli](https://github.com/coucagog/cloison-cli) | Apache-2.0 |
| Wrapper WASM | [coucagog/cloison-wasm](https://github.com/coucagog/cloison-wasm) | Apache-2.0 |
| Sidecar NER | [coucagog/cloison-detect](https://github.com/coucagog/cloison-detect) | Apache-2.0 |
| Harnais de bench | [coucagog/cloison-bench](https://github.com/coucagog/cloison-bench) | Apache-2.0 |

Adaptations par sous-arbre (commit de publication) : `Cargo.toml` autonome (les
valeurs héritées du workspace sont inlinées), dépendances internes en **git deps
épinglées** (`tag = "v0.1.0"`), texte de licence ajouté, README public. Le
`cloison-corpus` reste **privé** (jamais publié).

**Vérification** (voir `journal/DEPLOY-7.md`) : portes de sécurité (0 secret /
0 PII dans l'historique complet), `cargo test`/`clippy`/`fmt` par sous-arbre
(workspace équivalent, path deps), pytest detect (71) + bench (32), puis re-test
des dépôts publiés eux-mêmes (git deps réelles depuis GitHub).

Procédure (référence, réexécutable pour les versions suivantes) :

1. `git subtree split --prefix=<composant>` → branche de publication ;
2. création du dépôt public `coucagog/cloison-<composant>` (GitHub API) ;
3. push de la branche + tag `v0.1.0` (ordre de dépendance : core/ledger →
   audit → verify/control/proxy → cli/wasm/detect/bench) ;
4. vérification : `cargo test` sur chaque sous-arbre (git deps épinglées) ;
5. README public + lien vers la vérification WASM du journal.

**Versions publiées** : `v0.1.0` (DEPLOY-7) → `v0.2.0` (DEPLOY-8/9 : ONNX,
71/75) → `v0.2.1` (DEPLOY-9 : couverture étendue) → `v0.2.2` (DEPLOY-10 :
72/79, matricule officiel, correctif graphe) → **`v0.2.3` (STACK-N0 :
core/audit/proxy — coffre persistant N0, passphrase locale fail-loud,
Policy::n0_for, mode N0 du proxy ; vérifié : core 72 tests, audit 34,
proxy 42 dont e2e_n0 5/5)** → **`v0.2.4` (STACK-N0V11 : core/audit/proxy —
alias intra-session R1-R7 + jauge quasi-id in-core (core), wiring session
N0 du proxy ; deps git taguées v0.2.4 ; vérifié : cargo test des tags
publiés, rust 1.97)**.

## 5. Où vivent les licences

- Racine : `LICENSE` (Apache-2.0, texte officiel — workspace).
- `LICENSE-AGPL-3.0` : texte AGPL-3.0 (passerelle uniquement).
- `crates/cloison-proxy/Cargo.toml` : `license = "AGPL-3.0"` (dérogation
  workspace, documentée dans le fichier).
- Tous les autres crates : `license.workspace = true` → Apache-2.0.

## 6. Conformité charte

- §5.1 : composition ouverte définie + licences tranchées + corpus privé. ✅
- §10 : séparation corpus garantie (rien d'exfiltré, le corpus n'est pas dans
  ce dépôt). ✅
- §14 : docs à jour au fil des STACK. ✅ (journal DEPLOY-5)
