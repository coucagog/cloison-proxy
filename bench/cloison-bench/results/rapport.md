# CLOISON STACK-1 Benchmark Report

## Summary

- **Total Documents**: 500
- **Total Gold Entities**: 2324
- **Total Predicted Entities**: 3465
- **Non-PII Specificity**: 55.00%

## Global Metrics

- **Macro F1**: 0.7668 (IC 95%: [0.7571, 0.7754])
- **Weighted F1**: 0.7572 (IC 95%: [0.7484, 0.7654])

## Per-Entity Metrics

| Entity | TP | FP | FN | Precision | Recall | F1 | F1 IC 95% | Weight |
|--------|----|----|----|-----------|--------|----|-----------|--------|
| PERSON | 540 | 619 | 202 | 0.4659 | 0.7278 | 0.5681 | [0.5430, 0.5921] | 0.3 |
| LOC | 647 | 803 | 0 | 0.4462 | 1.0000 | 0.6171 | [0.6020, 0.6306] | 0.2 |
| CNI | 177 | 0 | 0 | 1.0000 | 1.0000 | 1.0000 | [1.0000, 1.0000] | 0.25 |
| MAIL | 314 | 0 | 19 | 1.0000 | 0.9429 | 0.9706 | [0.9564, 0.9832] | 0.15 |
| TEL | 259 | 106 | 140 | 0.7096 | 0.6491 | 0.6780 | [0.6428, 0.7098] | 0.1 |

## Per-Difficulty Metrics

### simple

- Documents: 160
- Gold entities: 399
- Predicted entities: 530

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.6278 | 0.8750 | 0.7311 |
| LOC | 0.4819 | 1.0000 | 0.6503 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9189 | 0.9577 |
| TEL | 0.7119 | 0.6774 | 0.6942 |

### contextual

- Documents: 160
- Gold entities: 681
- Predicted entities: 958

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4624 | 0.7688 | 0.5775 |
| LOC | 0.4880 | 1.0000 | 0.6559 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9244 | 0.9607 |
| TEL | 0.7432 | 0.6875 | 0.7143 |

### adversarial

- Documents: 80
- Gold entities: 1244
- Predicted entities: 1878

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4315 | 0.6564 | 0.5207 |
| LOC | 0.4575 | 1.0000 | 0.6277 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9605 | 0.9798 |
| TEL | 0.6772 | 0.6045 | 0.6388 |

### non_pii

- Documents: 100
- Gold entities: 0
- Predicted entities: 99

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.0000 | 0.0000 | 0.0000 |
| LOC | 0.0000 | 0.0000 | 0.0000 |
| CNI | 0.0000 | 0.0000 | 0.0000 |
| MAIL | 0.0000 | 0.0000 | 0.0000 |
| TEL | 0.0000 | 0.0000 | 0.0000 |
