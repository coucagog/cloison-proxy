# CLOISON — DEPLOY-2 : Wiring edge→detect (B.1) + préchargement au boot (B.2)

> Journal de déploiement — campagne B de la recommandation (après DEPLOY-1).
> Suite directe : A (finitions DEPLOY-1) ✅ → B.1 (wiring edge→detect) ✅ →
> B.2 (preload boot) ✅ → C (surface journal public, à venir).

## Objectif

1. **B.1 — Wiring edge→detect** : le proxy (`cloison-proxy`) consomme le sidecar
   NER (`cloison-detect`) via `CLOISON_DETECT_URL` (REST `POST /detect`) ; les
   spans PERSON/LOC du sidecar (Presidio + GLiNER + afroxlmr — le fossé
   ouest-africain) sont fusionnés à la détection embarquée, le cœur restant la
   source de vérité.
2. **B.2 — Préchargement au boot** : `CLOISON_PRELOAD` devient effectif (les
   modèles sont chargés AVANT de servir — constat DEPLOY-1 : le préchargement
   n'avait lieu qu'en mode `--check`).

## Décisions

1. **Transport REST** (repli du contrat gRPC) : plus simple côté proxy,
   contrat JSON miroir du proto. `core_spans` envoyés **vides** — le core
   déduplique lui-même les chevauchements (validation stricte).
2. **Cœur = source de vérité** : `cloison-core::Engine::tokenize_with_extra`
   valide chaque span externe (type activé par la politique, bornes UTF-8,
   valeur **re-tranchée du texte**, aucun chevauchement) avant fusion.
   Nouveaux `DetectorKind::Person` / `Location` (tags `PE`/`LO`) — jamais
   émis par les détecteurs embarqués.
3. **Dégradation gracieuse** : sidecar indisponible/timeout → warn + détection
   embarquée seule (jamais d'erreur, jamais de blocage) — vérifié en e2e mock
   (detect absent pendant le script → PASS).
4. **Réseau interne sans egress → modèles pré-provisionnés** : le réseau
   `cloison-internal` (internal: true, THREAT-MODEL §3.1) ne permet AUCUN
   téléchargement au boot (constat : `Temporary failure in name resolution`
   vers HF). Les modèles (GLiNER, backbone mdeberta, afroxlmr) sont
   téléchargés dans le volume `/models` par un conteneur helper (réseau
   bridge, egress OK), puis `HF_HUB_OFFLINE=1` (posture prod : aucun egress).
5. **Versions épinglées (dérive constatée)** : le rebuild non-pinné installait
   `huggingface-hub 1.28` + `transformers 5.15` (incompatibles avec
   `gliner==0.2.12` — `_from_pretrained` sans `proxies`/`resume_download` →
   GLiNER inactif). Épinglé : `transformers==4.46.3`, `huggingface-hub==0.26.3`.
6. **torch CPU-only** : `torch==2.5.1+cpu` (pin) + `--extra-index-url` index
   PyTorch CPU dans le Dockerfile. Le bundle CUDA PyPI (~6 Go, nccl/cuDNN) a
   **rempli le disque du VPS** au premier rebuild (no space left on device) —
   inutile sans GPU. Image detect : 7,88 Go → **2,06 Go**.
7. **spaCy EN aligné sur `spacy_size`** : l'ancien mapping EN renvoyait
   `en_core_web_lg` pour md — modèle jamais fourni (seul `en_core_web_sm`
   téléchargé) → presidio retombait en regex+gazetteers **en silence**.
   Corrigé (`en_core_web_${size}`) + `SPACY_EN_MODEL=en_core_web_md`.
8. **Backbone GLiNER provisionné** : `microsoft/mdeberta-v3-base` requis par
   gliner 0.2.12 (charge le backbone après son propre config) — ajouté au
   script de provisionnement.

## Actions réalisées

| # | Action | Résultat |
|---|---|---|
| B.1a | `cloison-core` : `DetectorKind::Person/Location`, `tokenize_with_extra` (validation stricte) + 4 tests | ✅ |
| B.1b | `cloison-proxy` : `DetectClient` (REST), config `CLOISON_DETECT_URL`/`TIMEOUT`, helpers async, `AppState.detect` | ✅ |
| B.1c | compose : edge sur `cloison-internal` + env detect | ✅ |
| B.2 | `main.py` : préchargement au boot (`CLOISON_PRELOAD != none`) | ✅ |
| — | Tests : `cargo test --workspace` (0 échec), pytest detect (70/70), `clippy -D warnings` (0), fmt detect.rs | ✅ |
| — | Déploiement : images rebuildées, stack up, modèles provisionnés (6,3 Go volume) | ✅ |
| — | Preuves : NER direct (`PERSON 0.88` sur nom non-gazetteer), e2e mock 10/10, e2e réel 5/5, zéro sentinelle résiduelle | ✅ |

## Résultats

- **Preuve NER** : `POST /detect` (réseau interne) sur « Appelez Xolani Ndlovu
  au 77 123 45 67… » → `PERSON [8,21] score 0.8799` — un nom NON-gazetteer,
  indétectable par les détecteurs embarqués.
- **Wiring vivant** : log edge « wiring edge→detect actif (B.1) » + appels
  `/detect` observés dans les logs sidecar pendant une requête réelle.
- **Préchargement complet au boot** : `presidio: oracle chargé (fr,en,md)`,
  `gliner: modèle chargé`, `african: modèle chargé (afroxlmr)` — en ~20 s,
  mémoire au repos 3,3 Go / 7,6 (marge large, swap intact).
- **Anti-crash** : **0 événement OOM** sur toute la campagne (build, boots,
  e2e) — memwatch continu.
- **Non-régression** : e2e mock **10/10**, e2e réel **5/5**, restauration
  complète (nom/téléphone/email), aucun jeton résiduel.

## Constats / dette

- **Dérive rustfmt** : `cargo fmt --check` sous rustfmt 1.97 signale 304 diffs
  sur des fichiers NON touchés (routes/stream/upstream/verify…) — artefact de
  version (le repo a été formaté avec un rustfmt antérieur). À normaliser dans
  un commit dédié si la CI l'exige ; les fichiers B.1 sont fmt-propres.
- **Egress réseau interne** : toute évolution des modèles NER exige de
  re-provisionner le volume `/models` (script `download_models.py` à garder
  dans `deploy/` — à déplacer dans le repo).
- **C — surface journal public** (`journal.wonkom.ai`, lecture-seule,
  vérifiable) : étape suivante de la campagne (charte §8) — non entamée ici.
- `dsh.wonkom.ai` : hors périmètre CLOISON (ancien hôte abandonné, DNS mort).

## Porte de sortie (campagne B)

- [x] B.1 implémenté, testé (core + proxy), déployé, prouvé (NER + wiring).
- [x] B.2 préchargement effectif au boot, tous détecteurs chargés.
- [x] Aucun crash (0 OOM), aucune régression (e2e mock + réel verts).
- [x] Journal + push GitHub (DEPLOY-2 + commits B).
- [ ] C — surface journal public (prochaine campagne).
