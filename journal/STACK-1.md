# CLOISON — STACK-1 : Benchmark d'abord (GO/NO-GO)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Mesurer équitablement le **fossé de détection** entre un détecteur spécialisé ouest-africain
et une baseline **Presidio bien configurée**, sur un jeu 100 % synthétique sénégalais, avec
une grille de scoring **pré-enregistrée**. La porte GO/NO-GO de la charte : si la détection
cible ne bat pas la baseline forte sur PERSON/LOC/CNI, le produit n'est pas justifié.

**Statut : la baseline est mesurée. Le comparatif avec un détecteur cible n'existe pas
encore — la décision finale GO/NO-GO est donc suspendue à STACK-2.** Mais les résultats
de la baseline livrent déjà des conclusions structurantes (voir ci-dessous).

## Périmètre

**Dans :** jeu d'évaluation synthétique sénégalais (0 PII réelle), baseline Presidio forte
(FR + regex CNI + gazetteers), harnais de scoring (exact match, F1 par entité, bootstrap,
spécificité non-PII), grille pré-enregistrée, rapport.

**Hors :** tout code produit CLOISON (détection, tokenisation, coffre, proxy). Aucun
détecteur cible n'est implémenté ici.

## Décisions

1. **Grille pré-enregistrée** (`bench/cloison-bench/grille.json`) : critères GO fixés AVANT
   le run — macro-F1 ≥ baseline + 0.10, F1_PERSON ≥ baseline + 0.12, F1_CNI ≥ baseline + 0.08,
   les trois simultanément, significativité p<0.05. Non modifiables après observation.
2. **Baseline forte, pas un homme de paille** : Presidio Analyzer FR (`fr_core_news_md`),
   Email/Phone preset, PatternRecognizer CNI custom (regex 13 chiffres + **validation Luhn**),
   gazetteers PERSON/LOC **chargés depuis le générateur** (mêmes listes → équité), seuil 0.0
   (rappel maximal). Si CLOISON bat ÇA, c'est significatif.
3. **0 PII réelle** : noms, CNI, téléphones, emails, lieux 100 % synthétiques (listes
   plausibles + Luhn + préfixes opérateurs + domaines `.sn` fictifs). Vérifié par audit
   d'échantillon.
4. **Scoring strict** : exact match sur les spans (start/end/type), normalisation
   casse/diacritiques/espaces, chevauchement partiel = FP + FN (la règle la plus sévère,
   conforme à la grille).
5. **Répartition du jeu** : 500 docs = 160 simple + 160 contextual + 80 adversarial + 100
   non-PII (20 %), proportions 2:2:1 sur la partie PII. Seed 42.

## Ce qui a été construit

- `bench/cloison-bench/generator.py` — générateur synthétique (noms, CNI+Luhn, TEL +221,
  MAIL .sn, LOC, 3 niveaux de difficulté + non-PII, spans, hash SHA-256).
- `bench/cloison-bench/scoring.py` — exact match, P/R/F1 par entité, macro/weighted,
  bootstrap IC 95 %, spécificité non-PII, critères GO/NO-GO.
- `bench/cloison-bench/presidio_baseline.py` — baseline forte FR + CNI Luhn + gazetteers.
- `bench/cloison-bench/run_benchmark.py` — CLI (`--seed`, `--samples`, `--output`).
- `bench/cloison-bench/grille.json` — grille pré-enregistrée (source de vérité).
- `bench/cloison-bench/protocole.txt` — protocole figé.
- `bench/cloison-bench/test_benchmark.py` — 32 tests unitaires, tous verts.
- `results/` — dataset.jsonl (hash SHA-256), predictions.jsonl, rapport.json, rapport.md.

## Comment lancer / tester

```bash
cd bench/cloison-bench
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python -m spacy download fr_core_news_md
pytest test_benchmark.py            # 32 tests
python run_benchmark.py --seed 42 --samples 500 --output results/
```

## Résultats (baseline Presidio forte, seed 42, 500 docs)

| Entité | F1 | Précision | Rappel | IC 95 % F1 |
|---|---|---|---|---|
| PERSON | 0.518 | 0.413 | 0.695 | [0.492, 0.545] |
| LOC | 0.596 | 0.424 | **1.000** | [0.581, 0.610] |
| CNI | **1.000** | 1.000 | 1.000 | [1.000, 1.000] |
| MAIL | 0.985 | 1.000 | 0.970 | [0.974, 0.993] |
| TEL | 0.652 | 0.629 | 0.677 | [0.625, 0.678] |
| **Macro F1** | **0.750** | | | [0.742, 0.758] |
| **Weighted F1** | 0.738 | | | [0.729, 0.747] |
| Spécificité non-PII | 42 % | | | |

Analyse par difficulté : simple (F1 macro ~0.72), contextual (~0.76), adversarial (~0.75).
Sur les non-PII : 135 faux positifs sur 100 documents → spécificité 42 % (la baseline
« voit » de la PII partout — terrain favorable à un détecteur plus discriminant).

## Invariants de sécurité vérifiés

- **0 PII dans le jeu** : tout est synthétique (audit d'échantillon + revue QA).
- **Aucune donnée réelle dans le dépôt** : le dataset est généré, pas collecté.
- **Aucun secret** : `.env` exclus, token GitHub uniquement dans `~/.git-credentials` (600).
- **Reproductibilité** : seed 42, hash du dataset (`324a7612…dddc8`) dans le rapport.

## Résultats QA (revue adversarial, agent indépendant)

Verdict : **GO conditionnel** — 2 failles critiques corrigées :
1. **Parité Luhn générateur/validateur** : 1784/2000 CNI échouaient leur propre validation
   → corrigé (parité `i%2==0` sur corps inversé), re-testé 4000/4000 valides.
2. **Spans désynchronisées** avec placeholders multiples (`{PERSON}` ×5) → réécrit
   `_fill_template` avec offset cumulé (gauche→droite).
Moyennes corrigées : regex TEL exacte (préfixes 70/75/76/77/78), gazetteers importés depuis
le générateur (source unique), distribution du jeu conforme à la grille, spécificité
recalculée au niveau document (était négative : entités vs documents mélangés).
Découvertes en rodage : `NlpEngineProvider` requis pour le modèle FR (pas `model_name`),
`supported_languages=['fr']` au constructeur du registry, recognizers custom en `fr`
(le `en` par défaut les faisait filtrer), regex CNI alignée sur 13 chiffres (l'exemple de la
grille en montrait 12 — incohérence notée), mapping des types Presidio → grille
(EMAIL_ADDRESS→MAIL, PHONE_NUMBER→TEL, LOCATION→LOC) + filtrage des types hors grille.

## Questions ouvertes / dette

1. **Critère CNI inatteignable par construction** : la baseline est à F1_CNI = 1.000
   (regex + Luhn = détection parfaite sur du synthétique). Le critère « F1_CNI ≥ baseline
   + 0.08 » est mathématiquement impossible → **la grille doit être amendée** (v1.1) avant
   le comparatif STACK-2 : soit supprimer le critère CNI, soit le remplacer par un critère
   de robustesse (ex. faux positifs CNI sur non-PII), soit viser l'égalité CNI (≥ baseline)
   et concentrer le fossé sur PERSON/LOC. **À trancher avant STACK-2.**
2. **Le fossé potentiel est sur PERSON (0.52) et LOC (0.60)** — précisément les entités du
   corpus ouest-africain non commoditisé. La précision faible (0.41/0.42) indique un
   sur-détection (gazetteers + spaCy) : un détecteur plus discriminant a de la marge.
3. **Spécificité 42 %** : la baseline génère 135 FP sur 100 docs non-PII. Un critère de
   spécificité devrait entrer dans la grille v1.1.
4. **Répartition 160/160/80/100** (au lieu de 200/200/100/100 dans la grille) : le run
   standard est à 500 docs ; la grille supposait 600. Cohérent en proportions, à clarifier
   dans la v1.1 (500 docs = 200/160/80/60 ou 600 docs = 240/240/120/…).

## Porte de sortie

- [x] Jeu d'évaluation synthétique sénégalais généré (0 PII réelle, hash).
- [x] Baseline Presidio FORTE configurée et exécutée.
- [x] Grille de scoring pré-enregistrée (v1.0) + rapport avec métriques et IC.
- [x] Tests : 32/32 verts, invariants Luhn/roundtrip vérifiés.
- [x] Revue QA indépendante : failles critiques corrigées et re-testées.
- [ ] **GO/NO-GO final : SUSPENDU** — nécessite le détecteur cible (STACK-2) pour le
      comparatif, ET l'amendement de la grille (critère CNI) validé par MLS.

## Prochaine étape

**STACK-2 — `cloison-core` (Rust)** : détection déterministe, jetons HMAC+sel+somme,
registre d'émission, généralisation des faibles cardinalités, coffre chiffré, invariants
roundtrip, différentiel Presidio, build WASM. **Mais d'abord : valider avec MLS l'amendement
de la grille v1.1 (critère CNI + éventuel critère de spécificité) et le périmètre du
comparatif.** Sans grille valide, le GO/NO-GO ne veut rien dire.

---

## DÉCISION 2026-08-20 — Amendement grille v1.1 (validé par MLS, option 1)

**Problème constaté au run v1.0** : F1_CNI baseline = 1.000 (regex + Luhn sur synthétique =
parfait) → le critère « F1_CNI ≥ baseline + 0.08 » était mathématiquement inatteignable.

**Décision (option 1) :**
1. **CNI** : critère de dépassement supprimé → remplacé par **non-régression** (F1_CNI ≥ baseline).
2. **LOC** : nouveau seuil **+0.15** (baseline 0.596 — le corpus toponymique est le terrain du fossé).
3. **Spécificité non-PII** : nouveau critère **≥ 60 %** (baseline à 42 % — sur-détection).
4. PERSON **+0.12** et macro **+0.10** conservés.

**Grille v1.1 figée** (`bench/cloison-bench/grille.json`, version 1.1.0) : 5 conditions
simultanées pour GO. **Aucune autre modification de la grille après ce jour.**

**Valeurs baseline de référence** (gravées dans `results/rapport.json` → `baseline_ref`) :
macro 0.7501 · PERSON 0.5181 · LOC 0.5958 · CNI 1.0000 · spécificité 0.42.

**Porte STACK-1 mise à jour** : GO/NO-GO final toujours suspendu — il dépend du
comparatif STACK-2 (détecteur cible vs baseline_ref) sur la grille v1.1.
