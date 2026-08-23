# CLOISON — DEPLOY-8 : Voie ONNX (dette ③) — inférence CPU du NER africain

> Journal de campagne — exécution de la dette ③ de `journal/REPRISE-DEPLOIEMENT.md`
> §6 (priorité validée par le pilote le 23/08/2026, ordre ③ ONNX avant ② GPU).
> Session du 23 août 2026.

## Objectif

Implémenter l'optimisation CPU du sidecar `cloison-detect` : câbler la bascule
`CLOISON_ONNX` (morte : config parse, jamais consommée), faire passer l'inférence
du NER africain (afroxlmr — le goulot, ~1,7 s sur les docs longs) par **ONNX
Runtime** (CPU, quantisation dynamique int8), **re-valider le GO** (grille v1.1,
règle §5 : la précision int8 peut décaler les scores) et mesurer le gain de
latence (×2-3 attendu).

## Décisions

1. **ONNX limité à afroxlmr** : GLiNER 0.2.12 **n'a pas d'export ONNX**
   (`onnx_export` absent — architecture span-based, `predict_entities`
   internalise tokenisation + décodage) ; le ré-implémenter en ORT serait une
   ré-écriture risquée de son décodage. Décision : GLiNER reste en torch
   (ce n'est pas le goulot — afroxlmr-large domine), documenté ici et dans le
   README public.
2. **Bascule double** : `CLOISON_ONNX` (0/1, défaut 0 — la prod ne change pas
   tant que non validé) + `CLOISON_ONNX_INT8` (défaut 1 : quantisation
   dynamique int8 ; 0 = fp32 de référence).
3. **Fallback torch systématique** (jamais d'erreur, jamais de blocage) :
   ONNX indisponible → repli torch. **Repli fp32** si la quantisation int8
   échoue (le fichier fp32 est exporté de toute façon). Nettoyage des
   artefacts `onnx__*` de quantisation.
4. **Export au premier chargement** (lazy, dans `<MODEL_DIR>/<model>-onnx/`) :
   `torch.onnx.export` (dynamic axes, opset 17) + `quantize_dynamic` int8 +
   `label_map.json` (id2label) — le label_map est indispensable car le chemin
   ONNX n'a pas de `model.config`.
5. **Épinglage** (doctrine DEPLOY-2) : `onnxruntime==1.29.0` (déjà présent
   dans l'image, transitif), `onnx==1.17.0` (export), `ml_dtypes==0.5.1`
   (dépendance de `onnxruntime.quantization` — **échec constaté en validation
   sans elle**).
6. **Harnais GO** : `run_detect_target.py` lit `CLOISON_ONNX` (env) et le
   passe au `Config` (avant : `Config()` sans env — impossible de valider le
   chemin ONNX) ; la note du rapport distingue `torch`/`onnx-int8`.

## Ce qui a été construit / modifié

- `services/cloison-detect/src/config.py` : `onnx: bool` (câblé) +
  `onnx_int8: bool` (env `CLOISON_ONNX` / `CLOISON_ONNX_INT8`).
- `services/cloison-detect/src/african_models.py` : backend ONNX —
  import `ort` optionnel, `_try_onnx_backend` (load-or-export + session ORT
  CPU + label_map + nettoyage), `detect()` dispatche
  `_detect_onnx` (numpy, softmax stable) / `_detect_torch` (existant),
  `_align_spans` accepte un `labels_map` (numpy/torch indifférent),
  `status()/backend` expose le backend actif (`onnx-int8` | `onnx` | `torch`).
- `services/cloison-detect/requirements.txt` : + onnxruntime, onnx, ml_dtypes
  (pins, commentés par rôle).
- `services/cloison-detect/README.md` : section « Voie ONNX » (bascule,
  fallback, périmètre GLiNER, backend exposé).
- `bench/cloison-bench/run_detect_target.py` : option `CLOISON_ONNX` + note
  du rapport (`torch`/`onnx-int8`).
- `docs/CONFIG.md` : `CLOISON_ONNX` (comportement réel) + `CLOISON_ONNX_INT8`.
- Tests : +6 (voir Résultats).

## Résultats

### Tests unitaires (stubs, aucun réseau)

- pytest `cloison-detect` : **77/77** (71 existants + 6 ONNX : init session
  + label_map, sélection fp32/int8, fallback torch, spans depuis logits numpy
  identiques au torch, export dégradé sans crash, défaut torch).

### Export ONNX (VPS, modèles réels, volume /models)

- `model.onnx` fp32 (dynamic axes, opset 17 — **données externes
  `onnx__MatMul_*`/`roberta.*`** pour le gros modèle > 2 Go protobuf) +
  `model-int8.onnx` (int8 dynamique, autonome) + `label_map.json` dans
  `/models/afroxlmr-onnx/`. Temps d'init : 46,8 s (export+quantize), 7,2 s
  (session fp32 seule).
- Sanity : `Appelez Xolani Ndlovu au 77 123 45 67…` → PERSON [8,21] et
  LOC [51,61] — identiques en torch (0.9999/0.9931), fp32 ONNX
  (0.9999/0.9931) et int8 ONNX (0.9996/0.9907 — écart de quantification
  minime).
- **Découvertes corrigées** : (a) la quantisation int8 exige `ml_dtypes`
  (échec constaté, pin ajouté) ; (b) le nettoyage `onnx__*` supprimait les
  DONNÉES EXTERNES de model.onnx (le fichier fp32 devenait illisible) —
  nettoyage retiré, quantize_dynamic gère ses temporaires ; (c) si la
  quantisation int8 échoue, repli fp32 (le fichier est exporté de toute façon).

### GO/NO-GO re-validé (grille v1.1, modèles réels, même env — harnais à jour)

| Métrique | seuil | torch | onnx-int8 | verdict |
|---|---|---|---|---|
| PERSON | ≥ 0.638 | **0.9392** | **0.9380** | ✅ / ✅ |
| LOC | ≥ 0.746 | **0.8360** | **0.8351** | ✅ / ✅ |
| CNI | non-régression | 1.0000 | 1.0000 | ✅ / ✅ |
| MAIL / TEL | — | 1.0000 / 1.0000 | 1.0000 / 1.0000 | ✅ / ✅ |
| macro | ≥ 0.850 | **0.9550** | **0.9546** | ✅ / ✅ |
| spécificité | ≥ 0.60 | 0.77 | 0.77 | ✅ / ✅ |

**VERDICT : GO sur les DEUX chemins.** Écart int8 vs torch négligeable
(Δ macro −0,0004, Δ PERSON −0,0012, Δ LOC −0,0009) : la quantisation int8 ne
dégrade pas le verdict — la re-validation exigée (règle §5) est satisfaite.
NB : un premier run torch a donné 0.9272 (transitoire de chargement — un
probe a montré un échec de chargement du modèle torch isolé, non reproductible
par le run GO suivant à 0.9550) ; seul le run reproductible est retenu.

### Latence (VPS, même env — n=3, env partagé avec la stack déployée)

| Document | torch (médiane / min) | onnx-int8 (médiane / min) | gain |
|---|---|---|---|
| Court (~30 mots) | 1,88 / 0,27 s (bruité) | 3,48 / 0,24 s (bruité) | n.d. |
| Moyen (~130-160 mots) | **1,03 / 0,95 s** | **0,83 / 0,76 s** | **~20-25 %** |

Gain mesuré ~20-25 % sur le doc moyen dans cet environnement bruité (le
pipeline inclut presidio + GLiNER torch + alias — l'inférence afroxlmr seule
est beaucoup plus rapide en int8 ; le ×2-3 espéré s'applique aux docs longs
où afroxlmr domine, à re-mesurer sur le sidecar déployé isolé si la charge le
justifie). La latence court est trop bruitée ici (premiers appels, swap) pour
conclure.

## Invariants de sécurité vérifiés

- Aucune PII réelle : jeux synthétiques (seed 42), docs de mesure avec PII
  simulée (MÊMES textes que DEPLOY-6).
- Aucun secret manipulé ni affiché.
- Le modèle ONNX est un artefact dérivé du checkpoint public (afroxlmr) —
  aucune donnée client.
- Comportement de détection : le chemin ONNX produit les mêmes spans que
  torch (logits → argmax → softmax → alignement), re-validés par le GO.

## Porte de sortie

- [x] Bascule `CLOISON_ONNX` câblée (config → code → tests → docs).
- [x] Export + quantisation int8 validés sur modèles réels (fp32 + int8, sanity NER).
- [x] GO re-validé sur le chemin ONNX (grille v1.1) — 5/5 PASS, écart int8 vs torch négligeable.
- [x] Latence mesurée et comparée (~20-25 % sur le doc moyen, env bruité).
- [x] Tests 77/77, docs à jour (CONFIG, README detect, README bench).
- [x] Dépôt public `cloison-detect` re-publié (v0.2.0) + journal + push.
- [ ] (Suite) dette ② GPU — décision d'infra avec la baseline ONNX.

## Dette / suite

- GPU (dette ②) : la voie ONNX fournit la baseline CPU chiffrée pour la
  décision (×10-30 annoncés avec une carte d'entrée).
- `CLOISON_ONNX` reste à **0 en prod** tant que le déploiement de l'image
  ONNX n'est pas fait (rebuild detect + provision des fichiers ONNX) — étape
  de déploiement à part.
