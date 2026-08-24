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
| TEL | Numéros de téléphone sénégalais — mobiles +221 (préfixes 70/71/72/75/76/77/78/79) **et fixes** (30/32/33/36, zone 8/9) | 0.10 |
| PASSPORT / PERMIS / MATRICULE | Identifiants contextuels (passeport, permis de conduire, matricule État/IPRES) — **hors grille GO** (mesurés, non scorés) | — |

> **Attribution TEL (confirmée — plan national ITU T02020000B8, soumission
> ARTP, posté 2023-11-29, + sources 2026)** : mobile 70 Expresso · 7211
> CSU/Hayo · 754-756 MVNO Promobile · 757 MVNO Origines · 76 Free/Yas
> (ex-Tigo) · 77/78 Orange · 790 ADIE ; fixe 30 Expresso · 32 Free · 338/339
> Orange · 3611 CSU/Hayo. **75 n'est PAS Free** (correction DEPLOY-9) ; le
> préfixe **71** (signalé pilote 08/2026) n'apparaît pas au plan ITU 2023 —
> conservé en couverture (invariant I1), attribution à confirmer ARTP.
>
> **Matricule État/IPRES (format confirmé sur listes officielles
> fonctionpublique.gouv.sn)** : 6 chiffres + 1 lettre de contrôle (A-Z sans
> I ni O), « 515808/G » ou « 734123F » — l'ancien « 8-11 chiffres » ne
> matchait jamais les matricules réels. Passeport (1-2 lettres + 7-8 chiffres,
> CEDEAO/ICAO) et permis (SN 009 numérisé depuis 04/01/2024) : formats
> observés, toujours à confirmer — détection contextuelle conservatrice.

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
> dernières re-validations (grille v1.1, modèles réels, baseline officielle
> macro 0.7501) :
> - **71/75 + fixes 30/32/33/36 (N3)** : torch macro **0.9542** · PERSON
>   **0.9387** · LOC **0.8320** · CNI/MAIL/TEL 1.000 · spécificité **76 %** ;
>   onnx-int8 macro **0.9520** · PERSON **0.9401** · LOC **0.8199**.
> (artefacts dans `results/go_nogo_final.*.json` — voir `journal/DEPLOY-9.md`).

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

# Voie ONNX (dette ③, DEPLOY-8) : inférence du NER africain via ONNX Runtime
# (CPU int8) — même grille, re-validation séparée :
CLOISON_OFFLINE=1 CLOISON_ONNX=1 python run_detect_target.py

# NER ouest-africain sélectionnable (défaut : afroxlmr — le défaut produit) :
# CLOISON_AFRICAN_MODEL=serengeti|afroxlmr|masakha  CLOISON_MIN_SCORE=0.40
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
                       # — mobiles 70-78 + fixes 30-36 + passeport/permis/matricule
scoring.py             # Module d'évaluation et métriques (exact match, bootstrap IC 95 %)
presidio_baseline.py   # Configuration Presidio forte (FR + CNI Luhn + gazetteers + contextuels)
run_benchmark.py       # CLI principal (baseline)
run_detect_target.py   # GO/NO-GO avec le détecteur cible (cloison-detect)
measure_clusters.py    # Analyse des clusters de scores (calibration des seuils)
differential.py        # Différentiel cloison-core vs Presidio (oracle, charte §5.2)
test_benchmark.py      # Tests unitaires (36)
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
pytest test_benchmark.py -v     # 36 tests (Luhn, roundtrip, spans, grille,
                                # fixes 30-36, passeport/permis/matricule)
```

## Dépendance sur cloison-detect

`run_detect_target.py` importe le pipeline de détection
([cloison-detect](https://github.com/coucagog/cloison-detect)) : ajouter son
répertoire racine au `PYTHONPATH` (voir ci-dessus). `differential.py` lance le
binaire `detect_cli` de [cloison-core](https://github.com/coucagog/cloison-core)
(`CLOISON_CORE_BIN` env pour le chemin).

## Licence

Apache-2.0 — voir [LICENSE](LICENSE).
