#!/usr/bin/env python3
"""
CLOISON STACK-1 Benchmark - CLI Principal

Orchestre l'exécution complète du benchmark:
1. Génération du dataset synthétique
2. Exécution de la baseline Presidio
3. Scoring et comparaison
4. Génération des rapports

Usage:
    python run_benchmark.py --seed 42 --samples 500 --output ./results
"""

import argparse
import json
import os
import sys
import hashlib
from datetime import datetime
from pathlib import Path
from typing import Dict, Optional

# Imports locaux
from generator import DatasetGenerator, validate_cni_luhn
from scoring import Scorer
from presidio_baseline import create_baseline_analyzer, detect_batch, PRESIDIO_AVAILABLE


# ==============================================================================
# CONFIGURATION
# ==============================================================================

DEFAULT_SEED = 42
DEFAULT_SAMPLES = 500
DEFAULT_OUTPUT = "./results"

# Poids des entités (depuis grille.json)
ENTITY_WEIGHTS = {
    'PERSON': 0.30,
    'LOC': 0.20,
    'CNI': 0.25,
    'MAIL': 0.15,
    'TEL': 0.10
}


# ==============================================================================
# FONCTIONS UTILITAIRES
# ==============================================================================

def calculate_file_hash(filepath: str) -> str:
    """Calcule le hash SHA-256 d'un fichier."""
    with open(filepath, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def ensure_dir(path: str) -> Path:
    """Crée un répertoire s'il n'existe pas."""
    p = Path(path)
    p.mkdir(parents=True, exist_ok=True)
    return p


def save_json(data: Dict, filepath: str):
    """Sauvegarde un dictionnaire en JSON."""
    with open(filepath, 'w', encoding='utf-8') as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


# ==============================================================================
# BENCHMARK RUNNER
# ==============================================================================

class BenchmarkRunner:
    """
    Orchestrateur du benchmark STACK-1.
    
    Workflow:
    1. Génère le dataset synthétique
    2. Exécute la baseline Presidio
    3. Calcule les métriques
    4. Génère les rapports
    """
    
    def __init__(
        self,
        seed: int = DEFAULT_SEED,
        output_dir: str = DEFAULT_OUTPUT
    ):
        """
        Initialise le runner.
        
        Args:
            seed: Graine aléatoire pour reproductibilité
            output_dir: Répertoire de sortie
        """
        self.seed = seed
        self.output_dir = ensure_dir(output_dir)
        self.timestamp = datetime.now().isoformat()
        
        # Composants
        self.generator = DatasetGenerator(seed=seed)
        self.scorer = Scorer(seed=seed)
        self.analyzer = None
        
        # Résultats
        self.documents = []
        self.predictions = []
        self.result = None
        self.dataset_hash = ""
        
    def step1_generate_dataset(self, n_samples: int) -> bool:
        """
        Étape 1: Génération du dataset synthétique.
        
        Args:
            n_samples: Nombre de documents à générer
            
        Returns:
            True si succès, False sinon
        """
        print(f"\n{'='*60}")
        print("STEP 1: Generating synthetic dataset")
        print(f"{'='*60}")
        print(f"  - Seed: {self.seed}")
        print(f"  - Samples: {n_samples}")
        
        try:
            self.documents = self.generator.generate(n_docs=n_samples)
            
            # Sauvegarder le dataset
            dataset_path = self.output_dir / "dataset.jsonl"
            self.dataset_hash = self.generator.save(self.documents, str(dataset_path))
            
            # Statistiques
            entity_counts = {}
            for doc in self.documents:
                for entity in doc.entities:
                    etype = entity.type
                    entity_counts[etype] = entity_counts.get(etype, 0) + 1
            
            print(f"  - Documents generated: {len(self.documents)}")
            print(f"  - Dataset hash: {self.dataset_hash[:16]}...")
            print(f"  - Entity distribution:")
            for etype in ['PERSON', 'LOC', 'CNI', 'MAIL', 'TEL']:
                count = entity_counts.get(etype, 0)
                print(f"      {etype}: {count}")
            
            # Compter les non-PII
            non_pii = sum(1 for doc in self.documents if doc.difficulty == 'non_pii')
            print(f"  - Non-PII documents: {non_pii} ({non_pii/len(self.documents)*100:.1f}%)")
            
            return True
            
        except Exception as e:
            print(f"  ERROR: Failed to generate dataset: {e}")
            return False
    
    def step2_run_baseline(self) -> bool:
        """
        Étape 2: Exécution de la baseline Presidio.
        
        Returns:
            True si succès, False sinon
        """
        print(f"\n{'='*60}")
        print("STEP 2: Running Presidio baseline")
        print(f"{'='*60}")
        
        if not PRESIDIO_AVAILABLE:
            print("  ERROR: Presidio not available. Install with: pip install presidio-analyzer")
            return False
        
        try:
            # Créer l'analyzer
            print("  - Creating analyzer...")
            self.analyzer = create_baseline_analyzer(
                spacy_model="fr_core_news_md",
                score_threshold=0.0
            )
            
            if self.analyzer is None:
                print("  ERROR: Failed to create analyzer")
                return False
            
            # Convertir les documents en dict
            docs_dict = [
                {
                    'doc_id': doc.doc_id,
                    'text': doc.text,
                    'entities': [
                        {'type': e.type, 'start': e.start, 'end': e.end, 'text': e.text}
                        for e in doc.entities
                    ],
                    'difficulty': doc.difficulty
                }
                for doc in self.documents
            ]
            
            # Exécuter la détection
            print("  - Running detection...")
            self.predictions = detect_batch(
                self.analyzer,
                docs_dict,
                text_key='text',
                language='fr',
                score_threshold=0.0
            )
            
            # Sauvegarder les prédictions
            predictions_path = self.output_dir / "predictions.jsonl"
            with open(predictions_path, 'w', encoding='utf-8') as f:
                for pred in self.predictions:
                    f.write(json.dumps(pred, ensure_ascii=False) + '\n')
            
            # Statistiques
            total_pred = sum(len(p['entities']) for p in self.predictions)
            print(f"  - Predictions saved: {predictions_path}")
            print(f"  - Total entities detected: {total_pred}")
            
            return True
            
        except Exception as e:
            print(f"  ERROR: Baseline execution failed: {e}")
            import traceback
            traceback.print_exc()
            return False
    
    def step3_score(self) -> bool:
        """
        Étape 3: Scoring et calcul des métriques.
        
        Returns:
            True si succès, False sinon
        """
        print(f"\n{'='*60}")
        print("STEP 3: Scoring and metrics calculation")
        print(f"{'='*60}")
        
        try:
            # Préparer les données pour le scorer
            gold_docs = [
                {
                    'doc_id': doc.doc_id,
                    'text': doc.text,
                    'entities': [
                        {'type': e.type, 'start': e.start, 'end': e.end, 'text': e.text}
                        for e in doc.entities
                    ],
                    'difficulty': doc.difficulty
                }
                for doc in self.documents
            ]
            
            # Calculer les métriques avec bootstrap
            print("  - Computing metrics with bootstrap (1000 iterations)...")
            self.result = self.scorer.bootstrap_ci(
                gold_docs,
                self.predictions,
                n_iterations=1000,
                confidence_level=0.95
            )
            
            # Afficher les résultats
            print(f"\n  Results:")
            print(f"  - Macro F1: {self.result.macro_f1:.4f} "
                  f"(IC 95%: [{self.result.macro_f1_ci[0]:.4f}, {self.result.macro_f1_ci[1]:.4f}])")
            print(f"  - Weighted F1: {self.result.weighted_f1:.4f} "
                  f"(IC 95%: [{self.result.weighted_f1_ci[0]:.4f}, {self.result.weighted_f1_ci[1]:.4f}])")
            print(f"\n  Per-entity F1:")
            for etype, metrics in self.result.entity_metrics.items():
                ci = self.result.entity_f1_ci.get(etype, [0.0, 0.0])
                print(f"    - {etype}: {metrics.f1:.4f} "
                      f"(P: {metrics.precision:.4f}, R: {metrics.recall:.4f}) "
                      f"[{ci[0]:.4f}, {ci[1]:.4f}]")
            
            print(f"\n  Non-PII specificity: {self.result.specificity:.2%}")
            
            return True
            
        except Exception as e:
            print(f"  ERROR: Scoring failed: {e}")
            import traceback
            traceback.print_exc()
            return False
    
    def step4_generate_reports(self) -> bool:
        """
        Étape 4: Génération des rapports.
        
        Returns:
            True si succès, False sinon
        """
        print(f"\n{'='*60}")
        print("STEP 4: Generating reports")
        print(f"{'='*60}")
        
        try:
            # Générer le rapport JSON
            report = self.scorer.generate_report(self.result)
            
            # Ajouter les métadonnées
            report['metadata'] = {
                'benchmark_id': 'CLOISON-STACK-1-v1.0',
                'timestamp': self.timestamp,
                'seed': self.seed,
                'total_documents': len(self.documents),
                'dataset_hash': self.dataset_hash
            }
            
            # Sauvegarder le rapport JSON
            report_json_path = self.output_dir / "rapport.json"
            self.scorer.save_report(report, str(report_json_path))
            print(f"  - JSON report: {report_json_path}")
            
            # Générer le rapport Markdown
            report_md = self.scorer.generate_markdown_report(self.result)
            report_md_path = self.output_dir / "rapport.md"
            with open(report_md_path, 'w', encoding='utf-8') as f:
                f.write(report_md)
            print(f"  - Markdown report: {report_md_path}")
            
            # Générer un résumé
            print(f"\n  Summary:")
            print(f"  - Total documents: {len(self.documents)}")
            print(f"  - Macro F1: {self.result.macro_f1:.4f}")
            print(f"  - Weighted F1: {self.result.weighted_f1:.4f}")
            print(f"  - Dataset hash: {self.dataset_hash[:16]}...")
            
            return True
            
        except Exception as e:
            print(f"  ERROR: Report generation failed: {e}")
            import traceback
            traceback.print_exc()
            return False
    
    def run(self, n_samples: int) -> bool:
        """
        Exécute le benchmark complet.
        
        Args:
            n_samples: Nombre de documents
            
        Returns:
            True si succès complet, False sinon
        """
        print(f"\n{'#'*60}")
        print(f"# CLOISON STACK-1 BENCHMARK")
        print(f"# Timestamp: {self.timestamp}")
        print(f"# Seed: {self.seed}")
        print(f"# Samples: {n_samples}")
        print(f"# Output: {self.output_dir}")
        print(f"{'#'*60}")
        
        success = True
        
        # Étape 1
        if not self.step1_generate_dataset(n_samples):
            success = False
        
        # Étape 2
        if success and not self.step2_run_baseline():
            success = False
        
        # Étape 3
        if success and not self.step3_score():
            success = False
        
        # Étape 4
        if success and not self.step4_generate_reports():
            success = False
        
        # Finalisation
        print(f"\n{'#'*60}")
        if success:
            print("# BENCHMARK COMPLETED SUCCESSFULLY")
        else:
            print("# BENCHMARK FAILED")
        print(f"{'#'*60}\n")
        
        return success


# ==============================================================================
# CLI
# ==============================================================================

def main():
    """Point d'entrée CLI."""
    parser = argparse.ArgumentParser(
        description='CLOISON STACK-1 Benchmark Runner',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    python run_benchmark.py
    python run_benchmark.py --seed 42 --samples 500 --output ./results
    python run_benchmark.py --samples 100 --output ./test_run
        """
    )
    
    parser.add_argument(
        '--seed',
        type=int,
        default=DEFAULT_SEED,
        help=f'Random seed for reproducibility (default: {DEFAULT_SEED})'
    )
    
    parser.add_argument(
        '--samples',
        type=int,
        default=DEFAULT_SAMPLES,
        help=f'Number of documents to generate (default: {DEFAULT_SAMPLES})'
    )
    
    parser.add_argument(
        '--output',
        type=str,
        default=DEFAULT_OUTPUT,
        help=f'Output directory (default: {DEFAULT_OUTPUT})'
    )
    
    args = parser.parse_args()
    
    # Créer et exécuter le runner
    runner = BenchmarkRunner(
        seed=args.seed,
        output_dir=args.output
    )
    
    success = runner.run(n_samples=args.samples)
    
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
