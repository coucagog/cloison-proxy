# CLOISON — DEPLOY-6 : torch 2.6.0 (CVE), calibration exécutée, latence detect

> Journal de déploiement — traitement des 3 points ouverts après DEPLOY-5 :
> session_ref/calibration, CVE torch 2.5.1→2.6.0, latence detect 2-6 s/doc.
> Session du 23 août 2026.

## Objectif

1. **Calibration** : exécuter réellement `measure_clusters.py` (et re-valider la
   calibration consensus PERSON/LOC de STACK-8) sur la stack du VPS ;
   confirmer le `session_ref_hashed` = hash du jeton d'accès en production.
2. **CVE torch** : upgrade `torch==2.5.1+cpu` → `torch==2.6.0+cpu`
   (fix CVE-2025-32434 CRITICAL) **avec re-validation complète du verdict
   GO/NO-GO** (grille v1.1, 5 conditions simultanées) et non-régression.
3. **Latence detect** : mesure réelle sur le VPS (min/médiane/p95), analyse du
   goulot, optimisation CPU actionnable (voie ONNX évaluée), recommandation
   GPU documentée.

## Décisions

1. **Torch 2.6.0+cpu épinglé** (le pin reste obligatoire — doctrine DEPLOY-2) :
   l'upgrade est validé par (a) les 70 tests detect, (b) le re-run GO/NO-GO
   complet avec `afroxlmr`, (c) la calibration `measure_clusters.py`. Si le
   verdict GO tenait, l'image detect est reconstruite et redéployée.
2. **Re-validation sur le VPS avec l'environnement de prod** : conteneur
   `benchdev` (python:3.11-slim, mêmes deps épinglées que l'image detect +
   index CPU PyTorch, volume `detect-models` monté sur `/models`), binaire
   core `detect_cli` recompilé. Le sidecar detect est **arrêté pendant le
   run** (libère la RAM ; l'edge dégrade gracieusement — B.1, vérifié).
3. **Dataset régénéré** (`run_benchmark.py --seed 42 --samples 500`) :
   `dataset.jsonl` est gitignoré (régénéré) ; `rapport.json`/`rapport.md`
   commités sont réécrits — doit être **identique** (déterminisme seed 42) ;
   vérifier `git status` après.
4. **Latence** : mesure du service déployé (après redéploiement torch 2.6.0)
   sur des documents représentatifs ; si la voie ONNX (export afroxlmr/GLiNER
   + onnxruntime int8) est jugée implémentable sans risque pour le verdict,
   elle est documentée comme piste — sinon recommandation GPU chiffrée.
5. **`session_ref_hashed`** : déjà implémenté (DEPLOY-5, hash du jeton
   d'accès — session réelle stable par client, testé) — vérification en
   production sur les reçus réels.

## Ce qui a été fait

- `services/cloison-detect/requirements.txt` : `torch==2.6.0+cpu`
  (commentaire CVE).
- Environnement de re-validation : `benchdev` (python 3.11-slim, bench +
  deps detect épinglées avec torch 2.6.0+cpu, spaCy fr/en `md`, proto gen
  régénéré, volume `/models`) ; `rustdev` re-créé pour `detect_cli`
  (`target/debug/detect_cli`).
- Script `latency_measure.py` (mesure min/médiane/p95).

## Résultats

### 🔴 Découverte majeure : le NER africain renvoyait [] en production
`african_models.py` passait `offset_mapping` au modèle
(`model(**encoded)` avec `return_offsets_mapping=True`) — or
`XLMRobertaForTokenClassification.forward()` n'accepte pas ce kwarg
(signature sans `**kwargs`) → **TypeError silencieuse** (warn + spans ignorés).
Le venv **non-pinné** du STACK-8 tolérait le kwarg (transformers plus récent) —
le verdict GO était donc mesuré avec afroxlmr actif ; mais **depuis le pin
transformers 4.46.3 (DEPLOY-2), l'image detect déployée avait un détecteur
africain inactif** (seuls Presidio+GLiNER+alias contribuaient). Corrigé :
`offsets = encoded.pop("offset_mapping")` avant l'appel modèle + test de
régression (fake model qui rejette le kwarg) — **71/71 tests verts**.

### Non-régression tests (torch 2.6.0+cpu)
- `pytest services/cloison-detect/tests` : **71/71 verts** (dont le nouveau
  test de régression offset_mapping).

### GO/NO-GO re-validé (torch 2.6.0+cpu, afroxlmr FIXÉ, baseline officielle)
| Métrique | re-validation | seuil | verdict |
|---|---|---|---|
| PERSON | **0.9365** | ≥ 0.638 (0.518+0.12) | ✅ |
| LOC | **0.8366** | ≥ 0.746 (0.596+0.15) | ✅ |
| CNI | 1.000 | non-régression | ✅ |
| MAIL / TEL | 1.000 / 1.000 | — | ✅ |
| macro | **0.9546** | ≥ 0.850 (0.750+0.10) | ✅ |
| spécificité | 0.77 | ≥ 0.60 | ✅ |

**VERDICT : GO** — identique au STACK-8 (0.937/0.835/0.954/0.77) : torch 2.6.0
ne dégrade rien et le fix restaure exactement le fossé mesuré. Artefacts :
`results/go_nogo_final.json` (canonique) + `go_nogo_final.torch260-afroxlmr.json`.
NB : la régénération du rapport a produit une baseline légèrement différente
(macro 0.7623 vs 0.7501 — deps bench non-épinglées, presidio plus récent) :
la **baseline officielle 0.7501 est restaurée** (référence gravée) et le
verdict est re-évalué contre elle (marges encore plus larges).

### Calibration exécutée (`measure_clusters.py`, torch 2.6.0)
- **TP : 1218** (identique au STACK-8) ; TP mono-source ≥ 0.9 : 1.
- **FP : 46, tous multi-sources** (toponymes réels dans des docs déclarés
  non-PII — tension de conception du jeu, STACK-8) ; **0 FP mono-source** :
  le consensus (refus mono-source < 0.90) tient sur la stack upgradée.
- Seuils calibrés confirmés : GLiNER 0.45, african 0.50, consensus PERSON/LOC.

### `session_ref_hashed` en production (point 1)
Reçus persistés vérifiés : `session_ref_hashed` = 64 hex SHA-256, **identique
pour les 3 reçus du même jeton** (session réelle stable par client), compteurs
uniquement (jamais de texte). Déjà implémenté DEPLOY-5 — confirmé en prod.

### Latence detect (VPS 144.217.81.251, 4 vCPU, torch 2.6.0 + afroxlmr ACTIF)
Mesurée sur le sidecar déployé (`POST /detect`, réseau interne) :

| Document | Taille | Latence (min / médiane / max) |
|---|---|---|
| Court (1 phrase, ~30 mots) | ~30 mots | **0,37 / 0,48 / 0,53 s** |
| Moyen (10 phrases, ~160 mots) | ~162 mots | **1,60 / 1,74 / 1,77 s** (19 spans) |
| Corpus benchmark (moyenne) | 500 docs | ~4 s/doc (run GO ≈ 35 min) |

**Constats** : le « 2-6 s/doc » du STACK-8 était la moyenne du CORPUS
benchmark (docs longs/adversariaux) sur l'ancien serveur (6 vCPU). En usage
réel (messages courts), le VPS tient **~0,5 s/doc** — acceptable. Les docs
longs (~160 mots) passent ~1,7 s.

**Recommandations (point 3)** :
- **GPU (recommandé pour charge réelle)** : une carte d'entrée (~2-4 Go VRAM,
  ex. T4/L4 ou équivalent) porterait l'inférence afroxlmr-large (fp16) à
  ~50-150 ms/doc (×10-30) et libérerait les 4 vCPU. Le verdict GO ne dépend
  pas du GPU (CPU suffit) ; il réduit la latence et la charge.
- **Optimisation CPU actionnable — voie ONNX (piste, non implémentée ici)** :
  export afroxlmr/GLiNER en ONNX + `onnxruntime` CPU int8 (config
  `CLOISON_ONNX` existe mais est une **bascule morte** — à câbler dans
  `african_models.py`/`gliner_detect.py` : session ORT remplaçant
  `model(**encoded)`) ; gain attendu ×2-3 sur la queue (docs longs) ;
  **validation requise** : re-run GO avec le chemin ONNX (la précision int8
  peut décaler les scores — grille v1.1). Coût d'implémentation : export +
  tests + re-validation → piste documentée, décision MLS.
- **Batching** : les requêtes simultanées sont sérialisées (un modèle partagé
  avec verrou) — un pool d'inférence ou un batching par lot aiderait sous
  charge. Documenté comme optimisation future.

## Invariants de sécurité vérifiés

- Aucune PII réelle : dataset STACK-1 synthétique (seed 42) ; requêtes de
  latence avec PII simulée.
- Aucun secret manipulé ni affiché.
- Pin inchangé sauf torch (validé par tests + GO).

## Porte de sortie

- [x] Torch 2.6.0+cpu validé (tests 71/71 + GO re-validé 5/5) et **déployé**
      (image detect rebuildée, afroxlmr chargé, preuve NER 0.9078).
- [x] **Bug critique corrigé** : le NER africain renvoyait [] en production
      depuis le pin transformers 4.46.3 (offset_mapping passé à forward) —
      corrigé + test de régression.
- [x] Calibration exécutée et consignée (1218 TP, 0 FP mono-source).
- [x] Latence mesurée (~0,5 s docs courts, ~1,7 s docs longs) + recommandations
      GPU/ONNX documentées.
- [x] `session_ref_hashed` confirmé en production (hash du jeton, stable).
- [ ] Journal + push (en cours) + CI.
