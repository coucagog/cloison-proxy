# CLOISON STACK-1 Benchmark

Harnais de benchmark pour mesurer le fossé de détection PII entre un détecteur ouest-africain et une baseline Presidio bien configurée, sur un jeu de données 100% synthétique sénégalais.

## Entités supportées

| Entité | Description | Poids |
|--------|-------------|-------|
| PERSON | Noms sénégalais (prénom + patronyme) | 0.30 |
| LOC | Toponymes du Sénégal (14 régions, villes, quartiers) | 0.20 |
| CNI | Carte Nationale d'Identité (13 chiffres, commençant par 1, checksum Luhn) | 0.25 |
| MAIL | Adresses email | 0.15 |
| TEL | Numéros de téléphone sénégalais (+221, préfixes 70/76/77/78) | 0.10 |

## Installation

```bash
# Créer un environnement virtuel
python -m venv venv
source venv/bin/activate  # Linux/macOS
# ou: venv\Scripts\activate  # Windows

# Installer les dépendances
pip install -r requirements.txt

# Télécharger le modèle spaCy français
python -m spacy download fr_core_news_md
```

## Utilisation

### Lancer le benchmark complet

```bash
# Avec les paramètres par défaut (seed=42, 500 documents)
python run_benchmark.py

# Avec des paramètres personnalisés
python run_benchmark.py --seed 42 --samples 500 --output ./results
```

### Arguments CLI

| Argument | Défaut | Description |
|----------|--------|-------------|
| `--seed` | 42 | Graine aléatoire pour reproductibilité |
| `--samples` | 500 | Nombre de documents à générer |
| `--output` | `./results` | Répertoire de sortie |

### Fichiers générés

Après exécution, le répertoire de sortie contient:

```
results/
├── dataset.jsonl       # Dataset synthétique avec annotations gold
├── predictions.jsonl   # Prédictions de la baseline Presidio
├── rapport.json        # Rapport complet en JSON
└── rapport.md          # Rapport en Markdown
```

## Structure du projet

```
cloison-bench/
├── generator.py           # Générateur de dataset synthétique
├── scoring.py             # Module d'évaluation et métriques
├── presidio_baseline.py   # Configuration Presidio forte
├── run_benchmark.py       # CLI principal
├── test_benchmark.py      # Tests unitaires
├── requirements.txt       # Dépendances Python
└── README.md              # Ce fichier
```

## Dataset synthétique

### Niveaux de difficulté

1. **Simple** (32%): Documents courts (1-3 phrases), 1-2 entités max
2. **Contextual** (32%): Documents moyens (3-7 phrases), 3-6 entités entrelacées
3. **Adversarial** (16%): Documents complexes (5-15 phrases), 5-10 entités, contextes pièges
4. **Non-PII** (20%): Documents sans PII pour mesurer la spécificité

### Format JSONLines

```json
{
  "doc_id": "doc_0001",
  "text": "Mamadou Diop réside à Dakar...",
  "entities": [
    {"type": "PERSON", "start": 0, "end": 12, "text": "Mamadou Diop"},
    {"type": "LOC", "start": 23, "end": 28, "text": "Dakar"}
  ],
  "difficulty": "simple",
  "seed": 42
}
```

## Métriques

### Matching

- **Exact match strict**: positions start/end identiques + type correct
- **Normalisation**: lowercase, NFC pour diacritiques, espaces normalisés

### Scores calculés

- **Precision, Recall, F1** par type d'entité
- **macro-F1**: moyenne non pondérée des F1
- **weighted-F1**: moyenne pondérée par les poids de la grille
- **IC 95%**: intervalles de confiance par bootstrap (1000 itérations)

### Critères GO/NO-GO

Le projet CLOISON passe GO si:

1. macro_F1_global >= baseline_macro_F1 + 0.10
2. F1_PERSON >= baseline_F1_PERSON + 0.12
3. F1_CNI >= baseline_F1_CNI + 0.08

Les 3 critères doivent être remplis simultanément.

## Algorithme Luhn pour CNI

Les numéros CNI sénégalais synthétiques respectent:

- Format: 13 chiffres commençant par '1'
- Validation: checksum Luhn (mod 10)
- Formats: compact ou avec espaces (1XX XXX XXXX XX)

```python
from generator import generate_cni, validate_cni_luhn

cni_full, cni_formatted = generate_cni()
# cni_full: "1752345678017"
# cni_formatted: "175 234 5678 01"

is_valid = validate_cni_luhn(cni_full)  # True
```

## Reproductibilité

Le benchmark est entièrement reproductible avec:

- Graine fixe (défaut: 42)
- Hash SHA-256 du dataset
- Versions figées des dépendances

```bash
# Vérifier le hash du dataset
sha256sum results/dataset.jsonl
```

## Tests

```bash
# Lancer tous les tests
pytest test_benchmark.py -v

# Avec couverture
pytest test_benchmark.py --cov=. --cov-report=html
```

## Extension

### Ajouter un nouveau détecteur

```python
# Custom detector interface
def detect_pii(text: str) -> list[dict]:
    """
    Args:
        text: Texte à analyser
        
    Returns:
        List of {'type': str, 'start': int, 'end': int, 'text': str}
    """
    # Votre implémentation
    pass

# Utiliser avec le scorer
from scoring import Scorer

scorer = Scorer(seed=42)
result = scorer.score(gold_docs, your_predictions)
```

### Modifier les templates

Les templates de documents sont définis dans `generator.py`. Ajoutez vos templates dans les listes:

- `TEMPLATES_SIMPLE`
- `TEMPLATES_CONTEXTUAL`
- `TEMPLATES_ADVERSARIAL`
- `TEMPLATES_NON_PII`

## Licence

Ce code fait partie du projet CLOISON. Aucune donnée PII réelle n'est utilisée.

## Références

- Grille de scoring: `_stage/stack1/grille.json`
- Protocole: `_stage/stack1/protocole.txt`
- Presidio: https://github.com/microsoft/presidio
