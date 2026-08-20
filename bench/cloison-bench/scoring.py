#!/usr/bin/env python3
"""
CLOISON STACK-1 Benchmark - Module de Scoring

Évalue les performances de détection PII en comparant les prédictions
aux annotations gold standard.

Métriques:
- Precision, Recall, F1 par type d'entité
- macro-F1 (moyenne non pondérée)
- weighted-F1 (moyenne pondérée par les poids de la grille)
- Intervalles de confiance bootstrap 95%
- Application des critères GO/NO-GO

Matching: exact match strict (start/end/type)
Normalisation: casse, diacritiques, espaces
"""

import json
import random
import unicodedata
from typing import List, Dict, Tuple, Optional
from dataclasses import dataclass, asdict, field
from collections import defaultdict
import numpy as np


# ==============================================================================
# POIDS DES ENTITÉS (depuis grille.json)
# ==============================================================================

ENTITY_WEIGHTS = {
    'PERSON': 0.30,
    'LOC': 0.20,
    'CNI': 0.25,
    'MAIL': 0.15,
    'TEL': 0.10
}


# ==============================================================================
# NORMALISATION
# ==============================================================================

def normalize_text(text: str) -> str:
    """
    Normalise le texte pour la comparaison.
    
    Applique:
    - Lowercase
    - Normalisation NFC (diacritiques)
    - Réduction des espaces multiples
    - Suppression des espaces en début/fin
    """
    # Normalisation Unicode NFC (compose les diacritiques)
    text = unicodedata.normalize('NFC', text)
    
    # Lowercase
    text = text.lower()
    
    # Réduire les espaces multiples
    text = ' '.join(text.split())
    
    return text.strip()


def spans_match(pred: Dict, gold: Dict) -> bool:
    """
    Vérifie si deux spans correspondent exactement.
    
    Critères:
    - Même position start
    - Même position end
    - Même type d'entité
    
    La normalisation est appliquée au texte mais les positions
    sont comparées sur le texte original.
    """
    return (
        pred['start'] == gold['start'] and
        pred['end'] == gold['end'] and
        pred['type'] == gold['type']
    )


# ==============================================================================
# STRUCTURES DE DONNÉES
# ==============================================================================

@dataclass
class EntityMetrics:
    """Métriques pour un type d'entité."""
    type: str
    true_positives: int = 0
    false_positives: int = 0
    false_negatives: int = 0
    
    @property
    def precision(self) -> float:
        if self.true_positives + self.false_positives == 0:
            return 0.0
        return self.true_positives / (self.true_positives + self.false_positives)
    
    @property
    def recall(self) -> float:
        if self.true_positives + self.false_negatives == 0:
            return 0.0
        return self.true_positives / (self.true_positives + self.false_negatives)
    
    @property
    def f1(self) -> float:
        if self.precision + self.recall == 0:
            return 0.0
        return 2 * (self.precision * self.recall) / (self.precision + self.recall)


@dataclass
class ScoringResult:
    """Résultat complet du scoring."""
    # Métriques par entité
    entity_metrics: Dict[str, EntityMetrics] = field(default_factory=dict)
    
    # Métriques globales
    macro_f1: float = 0.0
    weighted_f1: float = 0.0
    
    # Intervalles de confiance (bootstrap)
    macro_f1_ci: Tuple[float, float] = (0.0, 0.0)
    weighted_f1_ci: Tuple[float, float] = (0.0, 0.0)
    entity_f1_ci: Dict[str, Tuple[float, float]] = field(default_factory=dict)
    
    # Statistiques du dataset
    total_documents: int = 0
    total_gold_entities: int = 0
    total_pred_entities: int = 0
    
    # Performance par difficulté
    difficulty_metrics: Dict[str, Dict] = field(default_factory=dict)
    
    # Non-PII spécificité
    non_pii_total: int = 0
    non_pii_false_positives: int = 0
    specificity: float = 1.0
    
    # Critères GO/NO-GO
    go_criteria_met: bool = False
    criteria_details: Dict = field(default_factory=dict)


# ==============================================================================
# SCORER
# ==============================================================================

class Scorer:
    """
    Évaluateur de détection PII.
    
    Compare les prédictions aux annotations gold et calcule
    les métriques de performance.
    """
    
    def __init__(self, seed: int = 42):
        """
        Initialise le scorer.
        
        Args:
            seed: Graine pour le bootstrap (reproductibilité)
        """
        self.seed = seed
    
    def load_dataset(self, filepath: str) -> List[Dict]:
        """Charge un dataset JSONLines."""
        documents = []
        with open(filepath, 'r', encoding='utf-8') as f:
            for line in f:
                if line.strip():
                    documents.append(json.loads(line))
        return documents
    
    def load_predictions(self, filepath: str) -> List[Dict]:
        """Charge les prédictions JSONLines."""
        return self.load_dataset(filepath)
    
    def _compare_document(
        self, 
        gold_doc: Dict, 
        pred_doc: Dict
    ) -> Tuple[Dict[str, EntityMetrics], int, int, str]:
        """
        Compare un document gold avec ses prédictions.
        
        Retourne:
        - Dict de métriques par entité
        - Nombre de faux positifs sur non-PII
        - Nombre de gold entities
        - Niveau de difficulté
        """
        gold_entities = gold_doc.get('entities', [])
        pred_entities = pred_doc.get('entities', [])
        difficulty = gold_doc.get('difficulty', 'unknown')
        
        # Initialiser les métriques
        metrics = {etype: EntityMetrics(type=etype) for etype in ENTITY_WEIGHTS.keys()}
        
        # Marquer les entités gold comme non trouvées
        gold_matched = [False] * len(gold_entities)
        
        # Pour chaque prédiction
        for pred in pred_entities:
            pred_type = pred.get('type', 'UNKNOWN')
            
            # Ignorer les types inconnus
            if pred_type not in metrics:
                continue
            
            # Chercher une correspondance exacte
            matched = False
            for i, gold in enumerate(gold_entities):
                if not gold_matched[i] and spans_match(pred, gold):
                    metrics[pred_type].true_positives += 1
                    gold_matched[i] = True
                    matched = True
                    break
            
            if not matched:
                metrics[pred_type].false_positives += 1
        
        # Compter les faux négatifs
        for i, gold in enumerate(gold_entities):
            if not gold_matched[i]:
                gold_type = gold.get('type', 'UNKNOWN')
                if gold_type in metrics:
                    metrics[gold_type].false_negatives += 1
        
        # Faux positifs sur non-PII : on marque le DOCUMENT comme contaminé
        # dès qu'au moins une entité y est prédite (niveau document, pas entité).
        non_pii_fp = 1 if (difficulty == 'non_pii' and len(pred_entities) > 0) else 0
        
        return metrics, non_pii_fp, len(gold_entities), difficulty
    
    def score(
        self,
        gold_docs: List[Dict],
        pred_docs: List[Dict]
    ) -> ScoringResult:
        """
        Calcule les métriques de scoring.
        
        Args:
            gold_docs: Documents avec annotations gold
            pred_docs: Documents avec prédictions
            
        Returns:
            ScoringResult avec toutes les métriques
        """
        # Vérifier la correspondance
        if len(gold_docs) != len(pred_docs):
            raise ValueError(
                f"Nombre de documents différent: gold={len(gold_docs)}, pred={len(pred_docs)}"
            )
        
        # Initialiser les métriques globales
        global_metrics = {etype: EntityMetrics(type=etype) for etype in ENTITY_WEIGHTS.keys()}
        
        # Stats par difficulté
        difficulty_stats = defaultdict(lambda: {
            'total_docs': 0,
            'total_gold': 0,
            'total_pred': 0,
            'tp': defaultdict(int),
            'fp': defaultdict(int),
            'fn': defaultdict(int)
        })
        
        # Stats non-PII
        non_pii_total = 0
        non_pii_fp = 0
        
        total_gold = 0
        total_pred = 0
        
        # Comparer chaque document
        for gold_doc, pred_doc in zip(gold_docs, pred_docs):
            metrics, fp, n_gold, difficulty = self._compare_document(gold_doc, pred_doc)
            
            # Accumuler les métriques globales
            for etype, m in metrics.items():
                global_metrics[etype].true_positives += m.true_positives
                global_metrics[etype].false_positives += m.false_positives
                global_metrics[etype].false_negatives += m.false_negatives
            
            # Stats par difficulté
            diff_stats = difficulty_stats[difficulty]
            diff_stats['total_docs'] += 1
            diff_stats['total_gold'] += n_gold
            diff_stats['total_pred'] += sum(
                m.true_positives + m.false_positives for m in metrics.values()
            )
            
            for etype, m in metrics.items():
                diff_stats['tp'][etype] += m.true_positives
                diff_stats['fp'][etype] += m.false_positives
                diff_stats['fn'][etype] += m.false_negatives
            
            # Non-PII
            if difficulty == 'non_pii':
                non_pii_total += 1
                non_pii_fp += fp
            
            total_gold += n_gold
            total_pred += sum(m.true_positives + m.false_positives for m in metrics.values())
        
        # Calculer les métriques par difficulté
        difficulty_metrics = {}
        for diff, stats in difficulty_stats.items():
            diff_result = {
                'total_docs': stats['total_docs'],
                'total_gold': stats['total_gold'],
                'total_pred': stats['total_pred'],
                'per_entity': {}
            }
            
            for etype in ENTITY_WEIGHTS.keys():
                tp = stats['tp'][etype]
                fp = stats['fp'][etype]
                fn = stats['fn'][etype]
                
                precision = tp / (tp + fp) if (tp + fp) > 0 else 0.0
                recall = tp / (tp + fn) if (tp + fn) > 0 else 0.0
                f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0.0
                
                diff_result['per_entity'][etype] = {
                    'precision': precision,
                    'recall': recall,
                    'f1': f1
                }
            
            difficulty_metrics[diff] = diff_result
        
        # Calculer macro-F1 et weighted-F1
        f1_scores = {etype: global_metrics[etype].f1 for etype in ENTITY_WEIGHTS.keys()}
        
        macro_f1 = np.mean(list(f1_scores.values()))
        
        weighted_f1 = sum(
            f1_scores[etype] * ENTITY_WEIGHTS[etype] 
            for etype in ENTITY_WEIGHTS.keys()
        )
        
        # Spécificité non-PII
        specificity = (non_pii_total - non_pii_fp) / non_pii_total if non_pii_total > 0 else 1.0
        
        # Créer le résultat
        result = ScoringResult(
            entity_metrics=global_metrics,
            macro_f1=macro_f1,
            weighted_f1=weighted_f1,
            total_documents=len(gold_docs),
            total_gold_entities=total_gold,
            total_pred_entities=total_pred,
            difficulty_metrics=difficulty_metrics,
            non_pii_total=non_pii_total,
            non_pii_false_positives=non_pii_fp,
            specificity=specificity
        )
        
        return result
    
    def bootstrap_ci(
        self,
        gold_docs: List[Dict],
        pred_docs: List[Dict],
        n_iterations: int = 1000,
        confidence_level: float = 0.95
    ) -> ScoringResult:
        """
        Calcule les intervalles de confiance par bootstrap.
        
        Args:
            gold_docs: Documents gold
            pred_docs: Documents prédits
            n_iterations: Nombre d'itérations bootstrap (défaut: 1000)
            confidence_level: Niveau de confiance (défaut: 0.95)
            
        Returns:
            ScoringResult avec intervalles de confiance
        """
        random.seed(self.seed)
        np.random.seed(self.seed)
        
        # Score initial
        result = self.score(gold_docs, pred_docs)
        
        # Collecter les scores bootstrap
        macro_f1_samples = []
        weighted_f1_samples = []
        entity_f1_samples = {etype: [] for etype in ENTITY_WEIGHTS.keys()}
        
        n_docs = len(gold_docs)
        indices = list(range(n_docs))
        
        for _ in range(n_iterations):
            # Échantillonnage avec remise
            sample_indices = [random.choice(indices) for _ in range(n_docs)]
            
            sample_gold = [gold_docs[i] for i in sample_indices]
            sample_pred = [pred_docs[i] for i in sample_indices]
            
            # Calculer le score sur l'échantillon
            sample_result = self.score(sample_gold, sample_pred)
            
            macro_f1_samples.append(sample_result.macro_f1)
            weighted_f1_samples.append(sample_result.weighted_f1)
            
            for etype, metrics in sample_result.entity_metrics.items():
                entity_f1_samples[etype].append(metrics.f1)
        
        # Calculer les IC
        alpha = 1 - confidence_level
        lower_percentile = (alpha / 2) * 100
        upper_percentile = (1 - alpha / 2) * 100
        
        result.macro_f1_ci = (
            np.percentile(macro_f1_samples, lower_percentile),
            np.percentile(macro_f1_samples, upper_percentile)
        )
        
        result.weighted_f1_ci = (
            np.percentile(weighted_f1_samples, lower_percentile),
            np.percentile(weighted_f1_samples, upper_percentile)
        )
        
        result.entity_f1_ci = {
            etype: (
                np.percentile(samples, lower_percentile),
                np.percentile(samples, upper_percentile)
            )
            for etype, samples in entity_f1_samples.items()
        }
        
        return result
    
    def apply_go_criteria(
        self,
        result: ScoringResult,
        baseline_macro_f1: float,
        baseline_f1_person: float,
        baseline_f1_loc: float,
        baseline_f1_cni: float,
        baseline_specificity: float
    ) -> ScoringResult:
        """
        Applique les critères GO/NO-GO de la grille v1.1 (option 1 validée).

        Critères (les 5 simultanément pour GO):
        1. macro_F1_global >= baseline + 0.10
        2. F1_PERSON   >= baseline + 0.12
        3. F1_LOC      >= baseline + 0.15
        4. F1_CNI      >= baseline (NON-RÉGRESSION — baseline à 1.0, aucun
           dépassement exigé : inatteignable par construction, amendement v1.1)
        5. spécificité non-PII >= 0.60

        Args:
            result: Résultat du scoring (pour le système testé)
            baseline_macro_f1: macro-F1 de la baseline
            baseline_f1_person: F1 PERSON de la baseline
            baseline_f1_loc: F1 LOC de la baseline
            baseline_f1_cni: F1 CNI de la baseline
            baseline_specificity: spécificité non-PII de la baseline

        Returns:
            ScoringResult avec critères évalués
        """
        criteria = {
            'macro_f1_improvement': {
                'threshold': 0.10,
                'baseline_value': baseline_macro_f1,
                'required_value': baseline_macro_f1 + 0.10,
                'actual_value': result.macro_f1,
                'improvement': result.macro_f1 - baseline_macro_f1,
                'met': result.macro_f1 >= baseline_macro_f1 + 0.10
            },
            'person_f1_improvement': {
                'threshold': 0.12,
                'baseline_value': baseline_f1_person,
                'required_value': baseline_f1_person + 0.12,
                'actual_value': result.entity_metrics['PERSON'].f1,
                'improvement': result.entity_metrics['PERSON'].f1 - baseline_f1_person,
                'met': result.entity_metrics['PERSON'].f1 >= baseline_f1_person + 0.12
            },
            'loc_f1_improvement': {
                'threshold': 0.15,
                'baseline_value': baseline_f1_loc,
                'required_value': baseline_f1_loc + 0.15,
                'actual_value': result.entity_metrics['LOC'].f1,
                'improvement': result.entity_metrics['LOC'].f1 - baseline_f1_loc,
                'met': result.entity_metrics['LOC'].f1 >= baseline_f1_loc + 0.15
            },
            'cni_no_regression': {
                'threshold': 0.0,
                'baseline_value': baseline_f1_cni,
                'required_value': baseline_f1_cni,
                'actual_value': result.entity_metrics['CNI'].f1,
                'improvement': result.entity_metrics['CNI'].f1 - baseline_f1_cni,
                'met': result.entity_metrics['CNI'].f1 >= baseline_f1_cni
            },
            'specificity_threshold': {
                'threshold': 0.60,
                'baseline_value': baseline_specificity,
                'required_value': 0.60,
                'actual_value': result.specificity,
                'improvement': result.specificity - baseline_specificity,
                'met': result.specificity >= 0.60
            }
        }

        result.criteria_details = criteria
        result.go_criteria_met = all(c['met'] for c in criteria.values())

        return result

    def generate_report(self, result: ScoringResult) -> Dict:
        """
        Génère un rapport détaillé.
        
        Args:
            result: Résultat du scoring
            
        Returns:
            Dictionnaire avec le rapport complet
        """
        report = {
            'summary': {
                'macro_f1': result.macro_f1,
                'macro_f1_ci_95': list(result.macro_f1_ci),
                'weighted_f1': result.weighted_f1,
                'weighted_f1_ci_95': list(result.weighted_f1_ci),
                'total_documents': result.total_documents,
                'total_gold_entities': result.total_gold_entities,
                'total_pred_entities': result.total_pred_entities,
                'non_pii_specificity': result.specificity,
                'go_criteria_met': result.go_criteria_met
            },
            'per_entity': {},
            'per_difficulty': result.difficulty_metrics,
            'criteria': result.criteria_details
        }
        
        # Métriques par entité
        for etype, metrics in result.entity_metrics.items():
            report['per_entity'][etype] = {
                'true_positives': metrics.true_positives,
                'false_positives': metrics.false_positives,
                'false_negatives': metrics.false_negatives,
                'precision': metrics.precision,
                'recall': metrics.recall,
                'f1': metrics.f1,
                'f1_ci_95': list(result.entity_f1_ci.get(etype, [0.0, 0.0])),
                'weight': ENTITY_WEIGHTS[etype]
            }
        
        return report
    
    def save_report(self, report: Dict, filepath: str):
        """Sauvegarde le rapport en JSON."""
        with open(filepath, 'w', encoding='utf-8') as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
    
    def generate_markdown_report(self, result: ScoringResult, baseline_result: Optional[ScoringResult] = None) -> str:
        """
        Génère un rapport au format Markdown.
        
        Args:
            result: Résultat du scoring
            baseline_result: Résultat de la baseline (optionnel, pour comparaison)
            
        Returns:
            Rapport au format Markdown
        """
        md = []
        
        md.append("# CLOISON STACK-1 Benchmark Report")
        md.append("")
        md.append("## Summary")
        md.append("")
        md.append(f"- **Total Documents**: {result.total_documents}")
        md.append(f"- **Total Gold Entities**: {result.total_gold_entities}")
        md.append(f"- **Total Predicted Entities**: {result.total_pred_entities}")
        md.append(f"- **Non-PII Specificity**: {result.specificity:.2%}")
        md.append("")
        md.append("## Global Metrics")
        md.append("")
        md.append(f"- **Macro F1**: {result.macro_f1:.4f} (IC 95%: [{result.macro_f1_ci[0]:.4f}, {result.macro_f1_ci[1]:.4f}])")
        md.append(f"- **Weighted F1**: {result.weighted_f1:.4f} (IC 95%: [{result.weighted_f1_ci[0]:.4f}, {result.weighted_f1_ci[1]:.4f}])")
        md.append("")
        md.append("## Per-Entity Metrics")
        md.append("")
        md.append("| Entity | TP | FP | FN | Precision | Recall | F1 | F1 IC 95% | Weight |")
        md.append("|--------|----|----|----|-----------|--------|----|-----------|--------|")
        
        for etype, metrics in result.entity_metrics.items():
            ci = result.entity_f1_ci.get(etype, [0.0, 0.0])
            md.append(
                f"| {etype} | {metrics.true_positives} | {metrics.false_positives} | "
                f"{metrics.false_negatives} | {metrics.precision:.4f} | {metrics.recall:.4f} | "
                f"{metrics.f1:.4f} | [{ci[0]:.4f}, {ci[1]:.4f}] | {ENTITY_WEIGHTS[etype]} |"
            )
        
        md.append("")
        md.append("## Per-Difficulty Metrics")
        md.append("")
        
        for diff, stats in result.difficulty_metrics.items():
            md.append(f"### {diff}")
            md.append("")
            md.append(f"- Documents: {stats['total_docs']}")
            md.append(f"- Gold entities: {stats['total_gold']}")
            md.append(f"- Predicted entities: {stats['total_pred']}")
            md.append("")
            
            if stats['per_entity']:
                md.append("| Entity | Precision | Recall | F1 |")
                md.append("|--------|-----------|--------|----|")
                for etype, m in stats['per_entity'].items():
                    md.append(f"| {etype} | {m['precision']:.4f} | {m['recall']:.4f} | {m['f1']:.4f} |")
                md.append("")
        
        # Critères GO/NO-GO
        if result.criteria_details:
            md.append("## GO/NO-GO Criteria")
            md.append("")
            md.append("| Criterion | Threshold | Baseline | Required | Actual | Improvement | Met |")
            md.append("|-----------|-----------|----------|----------|--------|-------------|-----|")
            
            for name, criterion in result.criteria_details.items():
                status = "✓" if criterion['met'] else "✗"
                md.append(
                    f"| {name} | {criterion['threshold']:.2f} | "
                    f"{criterion['baseline_value']:.4f} | {criterion['required_value']:.4f} | "
                    f"{criterion['actual_value']:.4f} | {criterion['improvement']:.4f} | {status} |"
                )
            
            md.append("")
            md.append(f"**Decision**: {'GO' if result.go_criteria_met else 'NO-GO'}")
            md.append("")
        
        return '\n'.join(md)


# ==============================================================================
# POINT D'ENTRÉE
# ==============================================================================

if __name__ == "__main__":
    # Test du scorer avec des données fictives
    scorer = Scorer(seed=42)
    
    # Créer des données de test
    gold_docs = [
        {
            'doc_id': 'doc_001',
            'text': 'Mamadou Diop habite à Dakar.',
            'entities': [
                {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'},
                {'type': 'LOC', 'start': 23, 'end': 28, 'text': 'Dakar'}
            ],
            'difficulty': 'simple'
        },
        {
            'doc_id': 'doc_002',
            'text': 'La météo est belle aujourd\'hui.',
            'entities': [],
            'difficulty': 'non_pii'
        }
    ]
    
    # Prédictions (match parfait pour le premier doc)
    pred_docs = [
        {
            'doc_id': 'doc_001',
            'text': 'Mamadou Diop habite à Dakar.',
            'entities': [
                {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'},
                {'type': 'LOC', 'start': 23, 'end': 28, 'text': 'Dakar'}
            ]
        },
        {
            'doc_id': 'doc_002',
            'text': 'La météo est belle aujourd\'hui.',
            'entities': [
                # Faux positif
                {'type': 'LOC', 'start': 3, 'end': 8, 'text': 'météo'}
            ]
        }
    ]
    
    # Calculer le score
    result = scorer.bootstrap_ci(gold_docs, pred_docs, n_iterations=100)
    
    print(f"Macro F1: {result.macro_f1:.4f}")
    print(f"Weighted F1: {result.weighted_f1:.4f}")
    print(f"Specificity: {result.specificity:.2%}")
    
    # Générer le rapport
    report = scorer.generate_report(result)
    print(json.dumps(report, indent=2))
