# CLOISON — ARBITRAGE-04 : NER léger embarqué ONNX (chantier ④)

> Document d'arbitrage **pré-enregistré**, écrit AVANT toute mesure
> (26/08/2026) — esprit grille pré-enregistrée (STACK-1, grille v1.1).
> Références : `journal/N0V12-PREP.md` §2.1/§4.1 (critères à figer AVANT le
> run), charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§4 N0, §5.1 « ne force
> jamais un artefact unique à faire les deux », §6.1 couches 2-3, §11, §16,
> règle §5 de la grille v1.1), `docs/N0.md` §4.1 (la limite à lever).
> Les valeurs ci-dessous sont FIGÉES — non modifiables après observation.

## 1. Objet

Le daemon N0 (moteur Rust seul) a une limite assumée (docs/N0.md §4.1) : le
rappel PERSON/LOC en **texte libre** est réduit (un nom hors gazetteer et
jamais mentionné part en clair). Le chantier ④ évalue l'embarquement d'un
**NER léger** (PERSON/LOC) qui tourne **dans le processus du daemon** (jamais
un sidecar Python) pour lever cette limite.

## 2. Critères GO/NO-GO (FIGÉS avant le run)

Jeu : **STACK-1** (seed 42, 500 docs, synthétique — 0 PII réelle). Scoring :
`bench/cloison-bench/scoring.py` (exact-match strict start/end/type,
normalisation casse/diacritiques/espaces, spécificité au niveau document).
Pipeline mesuré : détection embarquée du core + **alias intra-session N0**
(+ NER léger pour le candidat). Types scorés : PERSON, LOC (+ CNI/MAIL/TEL
en non-régression).

**GO ⇔ les 5 conditions suivantes sont simultanément remplies :**

| # | Critère | Définition | Seuil |
|---|---|---|---|
| C1 | F1_PERSON | pipeline N0 + NER léger vs N0 actuel (sans NER) | **+ 0.10** (gain) |
| C2 | F1_LOC | idem | **+ 0.05** (gain) |
| C3 | Spécificité non-PII | docs non-PII sans FP | **≥ 0.60** ET **non-régression** vs N0 actuel |
| C4 | Non-régression structuré | CNI / MAIL / TEL (core déterministe) | **inchangé** (≥ N0 actuel) |
| C5 | Latence CPU doc court (~30 mots) | inférence NER embarquée seule, 4 vCPU VPS | **≤ 1.0 s** |

**NO-GO** = un critère manqué → report **documenté** (justification +
conditions de revue), la limite §4.1 reste assumée (aucune régression).

Re-validation grille v1.1 : **obligatoire** si le benchmark officiel
(pipeline serveur) est touché (règle §5). Le NER embarqué vit dans le
**proxy (mode N0 uniquement)** → le chemin serveur (`tokenize_with_extra`)
doit rester **bit-identique** (vérifié par les tests) → le GO/NO-GO officiel
de la stack serveur n'est pas re-mesuré, sauf si un test échoue.

## 3. Candidat modèle (choisi, FIGÉ)

| Attribut | Valeur |
|---|---|
| Modèle | `Davlan/bert-base-multilingual-cased-ner-hrl` (mBERT fine-tuné NER multilingue, MasakhaNER 2.0 + WikiANN — couvre le wolof, langues africaines) |
| Taille | ~178 Mo fp32 → **int8 dynamique ~60-90 Mo** (objectif ≤ 120 Mo) |
| Export ONNX | **officiel fourni par le repo HF** (`onnx/model.onnx` + `onnx/tokenizer.json` + vocab) — mécanique `quantize_dynamic` déjà maîtrisée (DEPLOY-8) |
| Licence | **AFL-3.0** — artefact provisionné (jamais committé), notice documentée ; même logique que torch épinglé au verdict GO (déviation documentée) |
| Runtime | **`ort` (Rust, onnxruntime)** dans `cloison-proxy` — `load-dynamic` (lib provisionnée avec le daemon), dégradation gracieuse si modèle/lib absents (N0 v1 inchangé) |
| Build WASM | le core ne change pas → build WASM intact (le runtime vit dans le proxy, natif) |

## 4. Décision d'architecture (FIGÉE avant le run)

1. **Le NER léger est un producteur de spans dans `cloison-proxy` (mode N0)**
   — exactement le rôle du sidecar distant (B.1) mais en local : le proxy
   tokenise (crate `tokenizers`, BERT WordPiece), infère (`ort`), aligne les
   spans (portage de `_align_spans`), et les passe à
   `Engine::tokenize_session(extra)` — le core reste la **source de vérité**
   (validation stricte des spans, invariants inchangés).
2. **Fusion englobante (N0 uniquement)** : un span NER complet (ex.
   « Aminata Diop ») doit primer sur les spans gazetteer partiels qu'il
   englobe (« Aminata », « Diop ») — sinon `merge_extra_spans` les jetterait
   (chevauchement) et le NER n'apporterait rien. Implémentation : option de
   session N0 (`SessionOptions.enable_enclosing_ner_fusion`), **chemin
   serveur bit-identique** (vérifié par les tests de non-régression).
3. **Dégradation gracieuse obligatoire** : modèle absent/corrompu, lib
   absente, timeout → N0 v1 (gazetteers + alias), warn, jamais d'erreur.
4. **Artefacts jamais committés** : modèle + lib provisionnés (volume/chemin
   local) via un script dédié ; le dépôt ne contient que le code.

## 5. Mesures à exécuter (dans l'ordre)

1. **A — État N0 actuel** : pipeline N0 (core + alias, sans NER) sur le jeu
   STACK-1 → F1 PERSON/LOC, spécificité, CNI/MAIL/TEL (référence).
2. **B — N0 + NER léger** : idem + spans mBERT ONNX int8 fusionnés.
3. **Latence** : inférence seule, doc court (~30 mots) et moyen (~160 mots),
   n ≥ 3, médiane.
4. **Verdict** : C1–C5 → GO/NO-GO, consigné ici + dans `STACK-N0V12.md`.
5. Si GO → implémentation (proxy `light_ner.rs`, fusion englobante, tests,
   portes, docs, journal) ; open-core v0.2.5 (proxy + wasm) ; sinon report.

## 6. Gardes

- **Zéro PII réelle** : jeux synthétiques uniquement (seed 42) ; le modèle
  est un checkpoint public, aucune donnée client.
- **Zéro secret** : aucun credential manipulé ; la lib et le modèle sont des
  artefacts publics.
- **Grille v1.1 intacte** : baseline officielle 0.7501 gravée, critères non
  modifiés ; le NER embarqué est une mesure produit N0, hors grille serveur.
- **Invariants core intacts** : 17 invariants bloquants inchangés (le core
  ne change que par une option de fusion N0, chemin serveur bit-identique).

---
*Arbitrage pré-enregistré le 26/08/2026. Toute déviation à ce document doit
être journalisée avec justification.*
