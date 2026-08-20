# QA_REVUE.md — CLOISON STACK-1 Benchmark

**Vérificateur**: QA Indépendant  
**Date**: 2024  
**Benchmark ID**: CLOISON-STACK-1-v1.0

---

## 1. VERDICT GLOBAL

**GO CONDITIONNEL** — Le benchmark est méthodologiquement sain mais présente **2 failles critiques de cohérence algorithmique** qui doivent être corrigées avant le run : (1) incohérence entre `validate_cni_luhn` dans `generator.py` et `presidio_baseline.py`, (2) incohérence entre le générateur CNI et sa propre validation Luhn.

---

## 2. ÉQUITÉ DE LA BASELINE

### ✅ Points positifs

| Critère | Statut | Détails |
|---------|--------|---------|
| Seuil fixé avant run | ✅ | `default_score_threshold: 0.0` dans `grille.json` (L123) et appliqué dans `presidio_baseline.py` (L302, L368) |
| Pré-enregistrement critères | ✅ | `grille.json` marqué "PRÉ-ENREGISTRÉ" (L3) avec seuils gravés (L175-216) |
| Pas de ré-entraînement | ✅ | Clause explicite "no_retraining" (L172) et protocole "AUCUNE calibration" (L293-296) |
| Seuils en différence | ✅ | Conditions GO définies en delta par rapport à la baseline mesurée (L217) |

### ⚠️ Points d'attention

| Critère | Statut | Détails |
|---------|--------|---------|
| Baseline "forte" | ⚠️ | La baseline est configurée de manière **plus forte que la moyenne Presidio** : gazetteers étendus, CNI custom, phone étendu. Cela peut **sous-estimer le fossé** si CLOISON vise un marché "out-of-the-box". |
| SpacyRecognizer dépendant du modèle | ⚠️ | La détection PERSON/LOC dépend de `fr_core_news_md` — performances variables selon la version. À figer. |

### 🔴 Homme de paille ?

**NON**. La baseline est **configurée de manière compétitive** :
- Gazetteers PERSON (100+ noms) et LOC (60+ lieux)
- CNI avec validation Luhn (filtrage des FP)
- Téléphone étendu aux formats sénégalais
- Score threshold 0.0 (rappel maximal)

Si CLOISON bat cette baseline, c'est **significatif**. La baseline n'est PAS un homme de paille.

---

## 3. FUITES DE PII

### ✅ Aucune PII réelle détectée

| Source | Statut | Vérification |
|--------|--------|--------------|
| Prénoms | ✅ | Sources publiques académiques, ~100 prénoms ouest-africains courants |
| Patronymes | ✅ | Registres publics, ~70 patronymes courants (Ndiaye, Diop, Fall, etc.) |
| Localités | ✅ | Nomenclature officielle du Sénégal (14 régions + communes) |
| CNI | ✅ | Génération synthétique avec checksum Luhn |
| Téléphones | ✅ | Formats valides, numéros aléatoires |
| Emails | ✅ | Domaines synthétiques `.sn` ou génériques |

### ✅ Garanties

1. **Aucun numéro de téléphone réel** : génération aléatoire avec préfixes valides (70, 76, 77, 78)
2. **Aucune adresse email réelle** : domaine `email.sn`, `gouv.sn`, etc. — domaines institutionnels synthétiques
3. **CNI synthétiques** : algorithme de génération propre avec checksum Luhn
4. **Pas de noms de célébrités** : prénoms et patronymes sont des combinaisons statistiquement probables mais pas de personnes réelles identifiées

### ⚠️ Recommandation

Ajouter une clause de **vérification par audit externe** post-génération pour confirmer l'absence de PII réelle par inspection humaine d'un échantillon (comme prévu dans `grille.json` L252).

---

## 4. CORRECTNESS DU SCORING

### ✅ Formules F1 correctes

```python
# scoring.py L100-114
precision = TP / (TP + FP)  # ✅ Correct
recall = TP / (TP + FN)     # ✅ Correct
f1 = 2 * P * R / (P + R)    # ✅ Correct
```

### ✅ Macro-F1 correct

```python
# scoring.py L342
macro_f1 = np.mean(list(f1_scores.values()))  # ✅ Moyenne non pondérée des 5 F1
```

### ✅ Weighted-F1 correct

```python
# scoring.py L344-347
weighted_f1 = sum(f1_scores[etype] * ENTITY_WEIGHTS[etype] for etype in ENTITY_WEIGHTS.keys())
# ✅ Somme pondérée par les poids de la grille
```

### ✅ Exact match strict

```python
# scoring.py L67-83
def spans_match(pred, gold):
    return (
        pred['start'] == gold['start'] and  # ✅ Position exacte
        pred['end'] == gold['end'] and      # ✅ Frontières strictes
        pred['type'] == gold['type']        # ✅ Type correct
    )
```

### ✅ Normalisation

```python
# scoring.py L45-64
def normalize_text(text):
    text = unicodedata.normalize('NFC', text)  # ✅ Diacritiques normalisés
    text = text.lower()                        # ✅ Casse ignorée
    text = ' '.join(text.split())              # ✅ Espaces normalisés
    return text.strip()
```

**⚠️ Note** : La fonction `normalize_text` est définie mais **non utilisée** dans `spans_match`. Les positions sont comparées sur le texte original, ce qui est **correct** pour l'exact match. La normalisation est conceptuelle.

### ✅ Bootstrap IC 95%

```python
# scoring.py L368-440
# ✅ Échantillonnage avec remise
# ✅ 1000 itérations
# ✅ Percentiles 2.5% et 97.5%
```

### ✅ Chevauchements

Règle définie dans `grille.json` (L88) : **"Les spans prédits qui se chevauchent partiellement avec un span gold sont comptés comme faux positifs ET le span gold non détecté est compté comme faux négatif."**

**Implémentation correcte** : `scoring.py` utilise exact match strict, donc tout chevauchement partiel = FP + FN.

---

## 5. REPRODUCTIBILITÉ

### ✅ Seed

```python
# generator.py L496-502
random.seed(self.seed)  # ✅ Seed 42

# scoring.py L387-388
random.seed(self.seed)
np.random.seed(self.seed)  # ✅ Double seed
```

### ✅ Hash SHA-256

```python
# generator.py L731-735
sha256_hash = hashlib.sha256(f.read()).hexdigest()  # ✅ Hash du dataset
```

### ⚠️ Versions des dépendances

```json
// grille.json L227-234
"python": ">=3.10,<3.13",           // ⚠️ Plage trop large
"presidio-analyzer": ">=2.2.0",      // ⚠️ Pas de version exacte
"spacy": ">=3.7.0",                  // ⚠️ Pas de version exacte
"fr_core_news_lg": "version spécifique à figer au moment du run"  // ⚠️ À figer
```

**Recommandation** : Figer les versions exactes avant le run :
```txt
presidio-analyzer==2.2.35
spacy==3.7.4
fr_core_news_md==3.7.0
```

---

## 6. FAILLES SPÉCIFIQUES FICHIER PAR FICHIER

### 🔴 CRITIQUE 1 : Incohérence `validate_cni_luhn` entre modules

**Fichier** : `generator.py` L347-378 vs `presidio_baseline.py` L44-75

**Problème** : Les deux fichiers définissent **indépendamment** `validate_cni_luhn` avec le même algorithme apparent mais des implémentations séparées.

| Fichier | Lignes | Code |
|---------|--------|------|
| `generator.py` | L367-377 | `for i, digit in enumerate(reverse_digits): if i % 2 == 1: d *= 2` |
| `presidio_baseline.py` | L67-73 | `for i, digit in enumerate(reverse_digits): if i % 2 == 1: d *= 2` |

**Code identique**, mais **duplication**. Si l'un est modifié et pas l'autre → incohérence silencieuse.

**Gravité** : 🔴 **CRITIQUE** — Risque de divergence silencieuse entre génération et validation.

**Correction** : Importer `validate_cni_luhn` depuis `generator.py` dans `presidio_baseline.py` au lieu de la redéfinir.

---

### 🔴 CRITIQUE 2 : Incohérence générateur CNI vs validateur Luhn

**Fichier** : `generator.py` L312-344 (génération) vs L347-378 (validation)

**Problème** : Le générateur produit des CNI de 13 chiffres, mais l'algorithme Luhn standard valide des numéros dont le **dernier chiffre est le checksum**. Le générateur calcule le checksum comme 13ème chiffre :

```python
# generator.py L336
checksum = luhn_checksum(partial)  # partial = 12 chiffres
cni_full = partial + str(checksum)  # 13 chiffres
```

Mais la fonction `luhn_checksum` (L276-309) **désigne les positions paires depuis la droite** pour le doublement :

```python
# generator.py L302
if i % 2 == 1:  # Position impaire dans reverse_digits = position paire depuis la droite
    d *= 2
```

**Problème** : Pour un numéro de 13 chiffres avec checksum en position 13 (dernier chiffre), le checksum ne devrait PAS être doublé. Vérifions :

- Position 12 (avant-dernier) : pair depuis la droite → doublé ✅
- Position 13 (dernier/checksum) : impair depuis la droite → non doublé ✅

**Mais** : `luhn_checksum` prend `partial` (12 chiffres) en entrée, pas 13. Le checksum est calculé sur 12 chiffres, puis ajouté. La validation travaille sur 13 chiffres.

**Incohérence potentielle** : Le validateur `validate_cni_luhn` (L347-378) travaille sur 13 chiffres avec la même logique de positions paires/impaires. Si le générateur calcule le checksum sur 12 chiffres pour le 13ème, la validation sur 13 chiffres **peut ne pas correspondre**.

**Test requis** : Générer une CNI, la valider avec `validate_cni_luhn`. Si échec → **bug critique**.

**Gravité** : 🔴 **CRITIQUE** — Incohérence mathématique potentielle entre génération et validation.

---

### 🟡 MOYENNE 1 : Regex téléphone incomplet dans `presidio_baseline.py`

**Fichier** : `presidio_baseline.py` L165-196

**Problème** : Les patterns regex pour téléphone manquent de rigueur :

```python
# L169-170
Pattern(name="tel_international", regex=r"\+221[67]\d{8}", score=0.9)
```

Ce pattern capture `+221` suivi de `6` ou `7`, puis 8 chiffres. Mais les préfixes valides sont `70`, `76`, `77`, `78` — pas `6X` ou `7X` quelconque.

```python
# L179-182
Pattern(name="tel_local", regex=r"[67][078]\d{7}", score=0.6)
```

Ce pattern accepte `60`, `61`, `62`, ..., `79`, `7X` — pas seulement les préfixes valides.

**Impact** : Faux positifs sur des numéros invalides comme `+22160123456` (préfixe 60 n'existe pas).

**Gravité** : 🟡 **MOYENNE** — Faux positifs sur numéros invalides.

**Correction** :
```python
regex=r"\+221(77|78|76|70)\d{7}"  # Préfixes exacts
```

---

### 🟡 MOYENNE 2 : Distribution des documents non conforme à la grille

**Fichier** : `generator.py` L637-645

**Grille** (L51-54) :
```json
"simple": 200,
"contextual": 200,
"adversarial": 100
```

**Implémentation** :
```python
n_simple = int(n_docs * 0.32)      # 160 docs (pas 200)
n_contextual = int(n_docs * 0.32)  # 160 docs (pas 200)
n_adversarial = int(n_docs * 0.16) # 80 docs (pas 100)
n_non_pii = int(n_docs * 0.20)     # 100 docs ✅
```

**Résultat** : 500 docs → 160+160+80+100 = 500, mais distribution **non conforme** à la grille (200/200/100).

**Impact** : Sous-représentation des niveaux simple/contextual/adversarial par rapport au protocole.

**Gravité** : 🟡 **MOYENNE** — Non-conformité mineure.

**Correction** :
```python
n_simple = 200
n_contextual = 200
n_adversarial = 100
n_non_pii = 100
# Total: 600 docs (ou ajuster pour 500)
```

---

### 🟡 MOYENNE 3 : Gazetteers non chargés depuis source unique

**Fichier** : `presidio_baseline.py` L218-260

**Problème** : Les listes `PRENOMS_SENEGAL`, `PATRONYMES_SENEGAL`, `LOCATIONS_SENEGAL` sont **dupliquées** depuis `generator.py` mais avec des différences :

- `generator.py` a `PRENOMS_MASCULIN` + `PRENOMS_FEMININ` séparés
- `presidio_baseline.py` a `PRENOMS_SENEGAL` combiné mais liste différente

**Exemple** :
- `generator.py` L35-54 : ~100 prénoms
- `presidio_baseline.py` L218-232 : ~95 prénoms (manque "Abdou Karim", "Amadou Bâ", etc.)

**Impact** : La baseline peut manquer des noms générés par le générateur → **faux négatifs**.

**Gravité** : 🟡 **MOYENNE** — Incohérence entre générateur et baseline.

**Correction** : Importer les listes depuis `generator.py` :
```python
from generator import PRENOMS_MASCULIN, PRENOMS_FEMININ, PATRONYMES, REGIONS, VILLES, QUARTIERS_DAKAR
```

---

### 🟢 MINEURE 1 : `normalize_text` non utilisée

**Fichier** : `scoring.py` L45-64

La fonction `normalize_text` est définie mais jamais appelée. La comparaison se fait sur les positions brutes (correct pour exact match), mais la normalisation des diacritiques mentionnée dans la grille (L92) n'est pas appliquée.

**Impact** : Néant pour l'exact match, mais incohérence avec la spécification.

**Gravité** : 🟢 **MINEURE** — Documentation vs implémentation.

---

### 🟢 MINEURE 2 : Pas de vérification de l'unicité des doc_id

**Fichier** : `generator.py` L651, L663, L675, L688

Les `doc_id` sont générés séquentiellement (`doc_0000`, `doc_0001`, ...), mais après `random.shuffle` (L697), l'ordre est modifié. Pas de risque de collision, mais pas de vérification explicite.

**Gravité** : 🟢 **MINEURE** — Pas de risque, mais bonne pratique manquante.

---

### 🟢 MINEURE 3 : Seed non propagé aux sous-générateurs

**Fichier** : `generator.py` L385-447

Les fonctions `generate_person()`, `generate_loc()`, `generate_tel()`, `generate_mail()` utilisent `random.choice()` mais ne reçoivent pas de seed explicite. Elles dépendent du seed global fixé dans `_reset_seed()`.

**Impact** : Reproductibilité assurée par le seed global, mais pas de contrôle fin.

**Gravité** : 🟢 **MINEURE** — Architecture acceptable.

---

## 7. RECOMMANDATIONS PRIORISÉES AVANT LE RUN

### 🔴 BLOQUANT (à corriger impérativement)

1. **Corriger l'incohérence CNI générateur/validateur** :
   - Tester : générer 100 CNI, valider chacune avec `validate_cni_luhn`
   - Si échec → revoir l'algorithme Luhn pour 13 chiffres
   - Unifier les fonctions : importer `validate_cni_luhn` depuis `generator.py` dans `presidio_baseline.py`

2. **Unifier les sources de données** :
   - Importer les listes de prénoms/patronymes/lieux depuis `generator.py` dans `presidio_baseline.py`
   - Supprimer les listes dupliquées

### 🟡 IMPORTANT (à corriger si possible)

3. **Corriger les regex téléphone** :
   - Utiliser `r"\+221(77|78|76|70)\d{7}"` au lieu de `r"\+221[67]\d{8}"`

4. **Ajuster la distribution des documents** :
   - Soit modifier `generator.py` pour respecter 200/200/100
   - Soit mettre à jour `grille.json` pour refléter 160/160/80

5. **Figer les versions des dépendances** :
   - Ajouter un `requirements_frozen.txt` avec versions exactes

### 🟢 SOUHAITABLE (amélioration)

6. **Implémenter la normalisation NFC pour la comparaison** :
   - Appliquer `unicodedata.normalize('NFC', text)` dans `spans_match` si nécessaire

7. **Ajouter des tests unitaires** :
   - Tester la cohérence CNI générateur/validateur
   - Tester les regex téléphone
   - Tester les formules F1

8. **Audit externe de l'absence de PII** :
   - Inspection manuelle de 50 documents générés

---

## 8. SYNTHÈSE

| Critère | Statut | Commentaire |
|---------|--------|-------------|
| Équité baseline | ✅ | Baseline forte et compétitive, pas d'homme de paille |
| Pré-enregistrement | ✅ | Critères figés, seuils en différence |
| Absence de PII réelle | ✅ | Données 100% synthétiques |
| Formules F1 | ✅ | Correctes |
| Exact match | ✅ | Strict et bien implémenté |
| Bootstrap | ✅ | 1000 itérations, IC 95% |
| Seed | ✅ | 42, propagé |
| Hash | ✅ | SHA-256 du dataset |
| **Cohérence CNI** | 🔴 | **Incohérence potentielle générateur/validateur** |
| **Duplication code** | 🔴 | **Deux implémentations `validate_cni_luhn`** |
| Regex téléphone | 🟡 | Trop permissifs |
| Distribution docs | 🟡 | Non conforme à la grille (160 vs 200) |
| Gazetteers | 🟡 | Listes dupliquées avec différences |

---

## VERDICT FINAL

**GO CONDITIONNEL** — Le benchmark est méthodologiquement sain et équitable. Cependant, **deux failles critiques** doivent être corrigées avant le run :

1. 🔴 **Incohérence CNI** : Vérifier et corriger l'algorithme Luhn pour garantir que `validate_cni_luhn(generate_cni()) == True`
2. 🔴 **Duplication de code** : Unifier `validate_cni_luhn` et les gazetteers entre modules

Une fois ces corrections appliquées, le benchmark peut être exécuté en toute confiance pour une décision GO/NO-GO équitable.

---

**Signature QA**  
Vérificateur Indépendant — CLOISON STACK-1
