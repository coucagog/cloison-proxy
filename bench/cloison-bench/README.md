# CLOISON Benchmark — harnais public GO/NO-GO

Harnais de benchmark pour mesurer le **fossé de détection PII** entre un détecteur
ouest-africain et une baseline Presidio bien configurée, sur un jeu de données
**100 % synthétique** sénégalais. Méthodologie **publique** (charte CLOISON §5.1) —
c'est la preuve que le produit se justifie.

Fait partie du projet [CLOISON](https://github.com/coucagog/cloison) — proxy de
confidentialité PII compatible OpenAI.

## Entités supportées

| Entité | Description | Poids |
|--------|-------------|-------|
| PERSON | Noms sénégalais (prénom + patronyme) | 0.30 |
| LOC | Toponymes du Sénégal (14 régions, villes, quartiers) | 0.20 |
| CNI | Carte Nationale d'Identité (13 chiffres, commençant par 1, checksum Luhn) | 0.25 |
| MAIL | Adresses email | 0.15 |
| TEL | Numéros de téléphone sénégalais (+221, préfixes 70/76/77/78) | 0.10 |

## Grille GO/NO-GO (v1.1 — pré-enregistrée, figée)

Le projet CLOISON passe **GO** si les 5 conditions suivantes sont **simultanément**
remplies (baseline Presidio forte, `results/rapport.json` → `baseline_ref`) :

| Condition | Seuil |
|---|---|
| F1_PERSON | ≥ baseline + 0.12 |
| F1_LOC | ≥ baseline + 0.15 |
| F1_CNI | non-régression (≥ baseline) |
| Macro F1 | ≥ baseline + 0.10 |
| Spécificité non-PII | ≥ 60 % |

> Vérification : `grille.json` (source de vérité). Verdict 2026 : **GO** —
> PERSON 0.937 · LOC 0.835 · CNI 1.000 · macro 0.954 · spécificité 0.77
> (artefacts dans `results/go_nogo_final.json` et `results/*.json`).

## Installation

```bash
python -m venv venv
source venv/bin/activate  # Linux/macOS ; ou: venv\Scripts\activate (Windows)
pip install -r requirements.txt
python -m spacy download fr_core_news_md
```

## Utilisation

```bash
# Baseline + harnais (seed 42, 500 documents)
python run_benchmark.py

# Détecteur cible complet (sidecar cloison-detect requis dans PYTHONPATH) :
#   git clone https://github.com/coucagog/cloison-detect
#   export PYTHONPATH=/chemin/cloison-detect
CLOISON_OFFLINE=1 python run_detect_target.py
```

### Fichiers générés

```
results/
├── dataset.jsonl       # Dataset synthétique avec annotations gold (gitignoré)
├── predictions.jsonl   # Prédictions (gitignoré)
├── rapport.json        # Rapport complet en JSON
└── rapport.md          # Rapport en Markdown
```

## Structure du projet

```
generator.py           # Générateur de dataset synthétique (seed 42, 0 PII réelle)
scoring.py             # Module d'évaluation et métriques (exact match, bootstrap IC 95 %)
presidio_baseline.py   # Configuration Presidio forte (FR + CNI Luhn + gazetteers)
run_benchmark.py       # CLI principal (baseline)
run_detect_target.py   # GO/NO-GO avec le détecteur cible (cloison-detect)
measure_clusters.py    # Analyse des clusters de scores (calibration des seuils)
differential.py        # Différentiel cloison-core vs Presidio (oracle, charte §5.2)
test_benchmark.py      # Tests unitaires (32)
grille.json            # Grille de scoring pré-enregistrée (source de vérité)
protocole.txt          # Protocole figé
```

## Dataset synthétique

1. **Simple** (32%) : documents courts (1-3 phrases), 1-2 entités max
2. **Contextual** (32%) : documents moyens (3-7 phrases), 3-6 entités entrelacées
3. **Adversarial** (16%) : documents complexes (5-15 phrases), 5-10 entités, contextes pièges
4. **Non-PII** (20%) : documents sans PII pour mesurer la spécificité

**Zéro PII réelle** : noms, CNI (Luhn), téléphones, emails, lieux 100 % synthétiques —
vérifié par audit d'échantillon (invariant CLOISON : le jeu n'est pas collecté, il est généré).

## Métriques

- **Exact match strict** : positions start/end identiques + type correct ;
  normalisation casse/diacritiques/espaces ; chevauchement partiel = FP + FN.
- **Precision, Recall, F1** par type d'entité ; **macro-F1** (moyenne non pondérée) ;
  **IC 95 %** par bootstrap (1000 itérations) ; **spécificité** non-PII au niveau document.

## Reproductibilité

- Graine fixe (seed 42) ; hash SHA-256 du dataset (`sha256sum results/dataset.jsonl`).
- Critères **pré-enregistrés** dans `grille.json` — non modifiables après observation.

## Tests

```bash
pytest test_benchmark.py -v     # 32 tests (Luhn, roundtrip, spans, grille)
```

## Dépendance sur cloison-detect

`run_detect_target.py` importe le pipeline de détection
([cloison-detect](https://github.com/coucagog/cloison-detect)) : ajouter son
répertoire racine au `PYTHONPATH` (voir ci-dessus). `differential.py` lance le
binaire `detect_cli` de [cloison-core](https://github.com/coucagog/cloison-core)
(`CLOISON_CORE_BIN` env pour le chemin).

## Licence

Apache-2.0 — voir [LICENSE](LICENSE).
