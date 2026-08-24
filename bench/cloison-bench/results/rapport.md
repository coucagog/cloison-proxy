# CLOISON STACK-1 Benchmark Report

## Summary

- **Total Documents**: 500
- **Total Gold Entities**: 2302
- **Total Predicted Entities**: 3437
- **Non-PII Specificity**: 50.00%

## Global Metrics

- **Macro F1**: 0.7743 (IC 95%: [0.7651, 0.7835])
- **Weighted F1**: 0.7618 (IC 95%: [0.7532, 0.7702])

## Per-Entity Metrics

| Entity | TP | FP | FN | Precision | Recall | F1 | F1 IC 95% | Weight |
|--------|----|----|----|-----------|--------|----|-----------|--------|
| PERSON | 541 | 615 | 191 | 0.4680 | 0.7391 | 0.5731 | [0.5486, 0.5976] | 0.3 |
| LOC | 611 | 778 | 0 | 0.4399 | 1.0000 | 0.6110 | [0.5947, 0.6285] | 0.2 |
| CNI | 179 | 0 | 0 | 1.0000 | 1.0000 | 1.0000 | [1.0000, 1.0000] | 0.25 |
| MAIL | 331 | 0 | 14 | 1.0000 | 0.9594 | 0.9793 | [0.9679, 0.9886] | 0.15 |
| TEL | 278 | 104 | 125 | 0.7277 | 0.6898 | 0.7083 | [0.6762, 0.7429] | 0.1 |

## Per-Difficulty Metrics

### contextual

- Documents: 160
- Gold entities: 686
- Predicted entities: 981

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4143 | 0.7250 | 0.5273 |
| LOC | 0.4897 | 1.0000 | 0.6574 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9350 | 0.9664 |
| TEL | 0.7448 | 0.6750 | 0.7082 |

### adversarial

- Documents: 80
- Gold entities: 1224
- Predicted entities: 1829

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4544 | 0.6893 | 0.5477 |
| LOC | 0.4520 | 1.0000 | 0.6226 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9785 | 0.9891 |
| TEL | 0.7288 | 0.6935 | 0.7107 |

### simple

- Documents: 160
- Gold entities: 392
- Predicted entities: 516

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.6409 | 0.8812 | 0.7421 |
| LOC | 0.4780 | 1.0000 | 0.6468 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9444 | 0.9714 |
| TEL | 0.6833 | 0.7193 | 0.7009 |

### non_pii

- Documents: 100
- Gold entities: 0
- Predicted entities: 111

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.0000 | 0.0000 | 0.0000 |
| LOC | 0.0000 | 0.0000 | 0.0000 |
| CNI | 0.0000 | 0.0000 | 0.0000 |
| MAIL | 0.0000 | 0.0000 | 0.0000 |
| TEL | 0.0000 | 0.0000 | 0.0000 |
