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

## 4. Publication (procédure, à décision MLS — non exécutée)

La publication réelle (création de dépôts **publics** sur GitHub, push des
sous-arbres) est un acte irréversible : elle attend la décision explicite de
MLS (moment de la « première release publique », charte §5.1/STACK-0). La
procédure est prête :

1. `git subtree split --prefix=crates/cloison-core` (et proxy, ledger, verify,
   audit, control, cli, wasm ; `services/cloison-detect` ; `bench/cloison-bench`)
   → branches de publication ;
2. création des dépôts publics `coucagog/cloison-<composant>` (GitHub API) ;
3. push des branches + tags `v0.1.0` ;
4. vérification : `cargo test` sur chaque sous-arbre (les crates ont des
   dépendances internes — publier `core` avant `audit`, `audit` avant
   `control`/`proxy`, `ledger` avant `control`/`verify` ; les `path`
   deviennent des versions crates.io ou des git deps épinglées) ;
5. mise à jour du README public + badge « vérifiable » (lien vers la
   vérification WASM du journal).

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
