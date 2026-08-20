# CLOISON STACK-1 Benchmark Report

## Summary

- **Total Documents**: 500
- **Total Gold Entities**: 2345
- **Total Predicted Entities**: 3737
- **Non-PII Specificity**: 42.00%

## Global Metrics

- **Macro F1**: 0.7501 (IC 95%: [0.7423, 0.7583])
- **Weighted F1**: 0.7375 (IC 95%: [0.7293, 0.7465])

## Per-Entity Metrics

| Entity | TP | FP | FN | Precision | Recall | F1 | F1 IC 95% | Weight |
|--------|----|----|----|-----------|--------|----|-----------|--------|
| PERSON | 516 | 734 | 226 | 0.4128 | 0.6954 | 0.5181 | [0.4921, 0.5445] | 0.3 |
| LOC | 636 | 863 | 0 | 0.4243 | 1.0000 | 0.5958 | [0.5813, 0.6102] | 0.2 |
| CNI | 182 | 0 | 0 | 1.0000 | 1.0000 | 1.0000 | [1.0000, 1.0000] | 0.25 |
| MAIL | 350 | 0 | 11 | 1.0000 | 0.9695 | 0.9845 | [0.9744, 0.9930] | 0.15 |
| TEL | 287 | 169 | 137 | 0.6294 | 0.6769 | 0.6523 | [0.6252, 0.6783] | 0.1 |

## Per-Difficulty Metrics

### contextual

- Documents: 160
- Gold entities: 684
- Predicted entities: 990

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4088 | 0.7000 | 0.5161 |
| LOC | 0.4932 | 1.0000 | 0.6606 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9778 | 0.9888 |
| TEL | 0.6250 | 0.6562 | 0.6402 |

### adversarial

- Documents: 80
- Gold entities: 1244
- Predicted entities: 1947

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4038 | 0.6469 | 0.4973 |
| LOC | 0.4389 | 1.0000 | 0.6100 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9679 | 0.9837 |
| TEL | 0.6311 | 0.6952 | 0.6616 |

### non_pii

- Documents: 100
- Gold entities: 0
- Predicted entities: 135

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.0000 | 0.0000 | 0.0000 |
| LOC | 0.0000 | 0.0000 | 0.0000 |
| CNI | 0.0000 | 0.0000 | 0.0000 |
| MAIL | 0.0000 | 0.0000 | 0.0000 |
| TEL | 0.0000 | 0.0000 | 0.0000 |

### simple

- Documents: 160
- Gold entities: 417
- Predicted entities: 665

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.5157 | 0.8187 | 0.6329 |
| LOC | 0.4280 | 1.0000 | 0.5995 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9487 | 0.9737 |
| TEL | 0.6341 | 0.6753 | 0.6541 |
