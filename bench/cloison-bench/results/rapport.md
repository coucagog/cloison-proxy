# CLOISON STACK-1 Benchmark Report

## Summary

- **Total Documents**: 500
- **Total Gold Entities**: 2345
- **Total Predicted Entities**: 3567
- **Non-PII Specificity**: 54.00%

## Global Metrics

- **Macro F1**: 0.7681 (IC 95%: [0.7597, 0.7769])
- **Weighted F1**: 0.7575 (IC 95%: [0.7498, 0.7662])

## Per-Entity Metrics

| Entity | TP | FP | FN | Precision | Recall | F1 | F1 IC 95% | Weight |
|--------|----|----|----|-----------|--------|----|-----------|--------|
| PERSON | 541 | 635 | 201 | 0.4600 | 0.7291 | 0.5641 | [0.5414, 0.5888] | 0.3 |
| LOC | 636 | 801 | 0 | 0.4426 | 1.0000 | 0.6136 | [0.5975, 0.6295] | 0.2 |
| CNI | 182 | 0 | 0 | 1.0000 | 1.0000 | 1.0000 | [1.0000, 1.0000] | 0.25 |
| MAIL | 350 | 0 | 11 | 1.0000 | 0.9695 | 0.9845 | [0.9744, 0.9930] | 0.15 |
| TEL | 287 | 135 | 137 | 0.6801 | 0.6769 | 0.6785 | [0.6476, 0.7068] | 0.1 |

## Per-Difficulty Metrics

### contextual

- Documents: 160
- Gold entities: 684
- Predicted entities: 983

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4286 | 0.7312 | 0.5404 |
| LOC | 0.4802 | 1.0000 | 0.6488 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9778 | 0.9888 |
| TEL | 0.6908 | 0.6562 | 0.6731 |

### adversarial

- Documents: 80
- Gold entities: 1244
- Predicted entities: 1863

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.4446 | 0.6754 | 0.5362 |
| LOC | 0.4602 | 1.0000 | 0.6303 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9679 | 0.9837 |
| TEL | 0.6736 | 0.6952 | 0.6842 |

### non_pii

- Documents: 100
- Gold entities: 0
- Predicted entities: 117

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
- Predicted entities: 604

| Entity | Precision | Recall | F1 |
|--------|-----------|--------|----|
| PERSON | 0.5816 | 0.8688 | 0.6967 |
| LOC | 0.5067 | 1.0000 | 0.6726 |
| CNI | 1.0000 | 1.0000 | 1.0000 |
| MAIL | 1.0000 | 0.9487 | 0.9737 |
| TEL | 0.6753 | 0.6753 | 0.6753 |
