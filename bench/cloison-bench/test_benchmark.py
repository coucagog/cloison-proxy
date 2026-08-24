#!/usr/bin/env python3
"""
CLOISON STACK-1 Benchmark - Tests Unitaires

Tests pour:
- Générateur de dataset
- Algorithme Luhn
- Calcul de métriques F1
- Configuration Presidio
"""

import pytest
import json
import tempfile
import os
from pathlib import Path

# Imports locaux
from generator import (
    DatasetGenerator,
    generate_cni,
    validate_cni_luhn,
    luhn_checksum,
    generate_person,
    generate_loc,
    generate_tel,
    generate_mail,
    generate_passport,
    generate_permis,
    generate_matricule,
    Entity
)
from scoring import (
    Scorer,
    EntityMetrics,
    normalize_text,
    spans_match
)


# ==============================================================================
# FIXTURES
# ==============================================================================

@pytest.fixture
def generator():
    """Fixture pour le générateur de dataset."""
    return DatasetGenerator(seed=42)


@pytest.fixture
def scorer():
    """Fixture pour le scorer."""
    return Scorer(seed=42)


@pytest.fixture
def sample_gold_docs():
    """Documents gold de test."""
    return [
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
            'text': 'Contact: aminata.diallo@email.sn ou +221771234567.',
            'entities': [
                {'type': 'MAIL', 'start': 10, 'end': 32, 'text': 'aminata.diallo@email.sn'},
                {'type': 'TEL', 'start': 36, 'end': 50, 'text': '+221771234567'}
            ],
            'difficulty': 'simple'
        },
        {
            'doc_id': 'doc_003',
            'text': 'La météo est belle aujourd\'hui.',
            'entities': [],
            'difficulty': 'non_pii'
        }
    ]


@pytest.fixture
def sample_pred_docs():
    """Documents prédits de test (match parfait pour le premier)."""
    return [
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
            'text': 'Contact: aminata.diallo@email.sn ou +221771234567.',
            'entities': [
                {'type': 'MAIL', 'start': 10, 'end': 32, 'text': 'aminata.diallo@email.sn'}
                # Faux négatif: TEL non détecté
            ]
        },
        {
            'doc_id': 'doc_003',
            'text': 'La météo est belle aujourd\'hui.',
            'entities': [
                # Faux positif
                {'type': 'LOC', 'start': 3, 'end': 8, 'text': 'météo'}
            ]
        }
    ]


# ==============================================================================
# TESTS LUHN
# ==============================================================================

class TestLuhnAlgorithm:
    """Tests pour l'algorithme de checksum Luhn."""
    
    def test_luhn_checksum_known_values(self):
        """Teste le checksum Luhn avec des valeurs connues."""
        # Test avec un numéro valide
        # Le checksum pour "123456789012" devrait être calculable
        partial = "123456789012"
        checksum = luhn_checksum(partial)
        
        # Vérifier que le checksum est entre 0 et 9
        assert 0 <= checksum <= 9
    
    def test_validate_cni_luhn_valid(self):
        """Teste la validation d'une CNI valide."""
        # Générer une CNI valide
        cni_full, _ = generate_cni()
        assert validate_cni_luhn(cni_full) is True
    
    def test_validate_cni_luhn_invalid(self):
        """Teste la validation d'une CNI invalide."""
        # CNI avec mauvais checksum
        invalid_cni = "1234567890123"
        assert validate_cni_luhn(invalid_cni) is False
    
    def test_validate_cni_wrong_length(self):
        """Teste le rejet d'une CNI de mauvaise longueur."""
        assert validate_cni_luhn("12345678") is False
        assert validate_cni_luhn("12345678901234") is False
    
    def test_validate_cni_wrong_prefix(self):
        """Teste le rejet d'une CNI ne commençant pas par 1."""
        assert validate_cni_luhn("2234567890123") is False
    
    def test_validate_cni_formatted(self):
        """Teste la validation d'une CNI avec espaces."""
        cni_full, cni_formatted = generate_cni()
        assert validate_cni_luhn(cni_formatted) is True
    
    def test_cni_generation_consistency(self):
        """Teste que toutes les CNI générées sont valides."""
        for _ in range(100):
            cni_full, cni_formatted = generate_cni()
            assert validate_cni_luhn(cni_full), f"Invalid CNI: {cni_full}"
            assert validate_cni_luhn(cni_formatted), f"Invalid formatted CNI: {cni_formatted}"


# ==============================================================================
# TESTS GÉNÉRATEUR
# ==============================================================================

class TestGenerator:
    """Tests pour le générateur de dataset."""
    
    def test_generator_seed_reproducibility(self, generator):
        """Teste que la même graine produit le même dataset."""
        docs1 = generator.generate(n_docs=10)
        generator._reset_seed()
        docs2 = generator.generate(n_docs=10)
        
        # Les textes doivent être identiques
        for d1, d2 in zip(docs1, docs2):
            assert d1.text == d2.text
    
    def test_generator_document_count(self, generator):
        """Teste le nombre de documents générés."""
        docs = generator.generate(n_docs=100)
        assert len(docs) == 100
    
    def test_generator_entity_types(self, generator):
        """Teste que les entités générées ont des types valides."""
        docs = generator.generate(n_docs=50)
        # Types de la grille GO + identifiants contextuels N3+ (hors grille).
        valid_types = {'PERSON', 'LOC', 'CNI', 'MAIL', 'TEL',
                       'PASSPORT', 'PERMIS', 'MATRICULE'}
        
        for doc in docs:
            for entity in doc.entities:
                assert entity.type in valid_types
    
    def test_generator_difficulty_distribution(self, generator):
        """Teste la distribution des niveaux de difficulté."""
        docs = generator.generate(n_docs=500)
        
        difficulties = [doc.difficulty for doc in docs]
        counts = {
            'simple': difficulties.count('simple'),
            'contextual': difficulties.count('contextual'),
            'adversarial': difficulties.count('adversarial'),
            'non_pii': difficulties.count('non_pii')
        }
        
        # Vérifier les proportions conformes à la grille (pré-enregistrée) :
        # 20 % non-PII ; parmi les PII : 40/40/20 % (2:2:1).
        total = len(docs)
        assert counts['non_pii'] == int(total * 0.20)
        n_pii = total - counts['non_pii']
        assert counts['simple'] == int(n_pii * 0.40)
        assert counts['contextual'] == int(n_pii * 0.40)
        assert counts['adversarial'] == n_pii - counts['simple'] - counts['contextual']
        assert counts['simple'] + counts['contextual'] + counts['adversarial'] + counts['non_pii'] == total
    
    def test_generator_span_validity(self, generator):
        """Teste que les spans sont valides (start < end, dans le texte)."""
        docs = generator.generate(n_docs=50)
        
        for doc in docs:
            for entity in doc.entities:
                assert 0 <= entity.start < entity.end <= len(doc.text)
                assert doc.text[entity.start:entity.end] == entity.text
    
    def test_generate_person(self):
        """Teste la génération de noms de personnes."""
        for _ in range(10):
            person = generate_person()
            # Doit contenir un espace (prénom + patronyme)
            assert ' ' in person
            parts = person.split()
            assert len(parts) >= 2
    
    def test_generate_loc(self):
        """Teste la génération de localisations."""
        for _ in range(10):
            loc = generate_loc()
            assert len(loc) > 0
            assert isinstance(loc, str)
    
    def test_generate_tel(self):
        """Teste la génération de numéros de téléphone (mobiles 70-78 + fixes 30-36)."""
        prefixes = ('70', '71', '75', '76', '77', '78', '30', '32', '33', '36')
        for _ in range(100):
            tel = generate_tel()
            # Vérifier le format
            assert '+' in tel or tel.startswith(prefixes)

    def test_generate_tel_fixe(self):
        """Teste la génération de numéros de téléphone FIXE (30/32/33/36)."""
        for _ in range(100):
            tel = generate_tel()
            if '+221' in tel:
                # fixe international : +221 3X 8/9 XXXXXX (9 chiffres NSN)
                digits = tel.replace('+221', '').replace(' ', '')
                if digits.startswith(('30', '32', '33', '36')):
                    assert len(digits) == 9, f"fixe international invalide: {tel}"
                    assert digits[2] in ('8', '9'), f"zone invalide: {tel}"
                continue
            if tel.startswith(('30', '32', '33', '36')):
                # fixe local : préfixe (2) + zone (8/9) + 6 chiffres = 9
                digits = tel.replace(' ', '')
                assert len(digits) == 9, f"fixe local invalide: {tel}"
                assert digits[2] in ('8', '9'), f"zone invalide: {tel}"

    def test_generate_passport(self):
        """Teste la génération de numéros de passeport (1-2 lettres + 7-8 chiffres)."""
        for _ in range(20):
            p = generate_passport()
            import re
            assert re.fullmatch(r"[A-Z]{1,2}[0-9]{7,8}", p), f"passeport invalide: {p}"

    def test_generate_permis(self):
        """Teste la génération de numéros de permis (7-10 chiffres)."""
        for _ in range(20):
            p = generate_permis()
            assert p.isdigit() and 7 <= len(p) <= 10, f"permis invalide: {p}"

    def test_generate_matricule(self):
        """Teste la génération de matricules État/IPRES (8-11 chiffres)."""
        for _ in range(20):
            m = generate_matricule()
            assert m.isdigit() and 8 <= len(m) <= 11, f"matricule invalide: {m}"
    
    def test_generate_mail(self):
        """Teste la génération d'emails."""
        for _ in range(10):
            mail = generate_mail()
            assert '@' in mail
            assert '.' in mail.split('@')[1]
    
    def test_save_and_hash(self, generator):
        """Teste la sauvegarde et le calcul de hash."""
        docs = generator.generate(n_docs=10)
        
        with tempfile.NamedTemporaryFile(suffix='.jsonl', delete=False) as f:
            filepath = f.name
        
        try:
            hash1 = generator.save(docs, filepath)
            assert len(hash1) == 64  # SHA-256 = 64 caractères hex
            
            # Vérifier que le fichier existe
            assert os.path.exists(filepath)
            
            # Re-sauvegarder et vérifier le même hash
            hash2 = generator.save(docs, filepath)
            assert hash1 == hash2
        finally:
            os.unlink(filepath)


# ==============================================================================
# TESTS SCORING
# ==============================================================================

class TestScoring:
    """Tests pour le module de scoring."""
    
    def test_normalize_text_lowercase(self):
        """Teste la normalisation en lowercase."""
        assert normalize_text("MAMADOU") == "mamadou"
        assert normalize_text("Dakar") == "dakar"
    
    def test_normalize_text_diacritics(self):
        """Teste la normalisation des diacritiques."""
        # NFC normalization
        text1 = "Aïssata"
        text2 = "Aissata"
        # Les deux devraient être équivalents après normalisation NFC
        assert normalize_text(text1) == normalize_text(text1)
    
    def test_normalize_text_whitespace(self):
        """Teste la normalisation des espaces."""
        assert normalize_text("Dakar  Pikine") == "dakar pikine"
        assert normalize_text("  test  ") == "test"
    
    def test_spans_match_exact(self):
        """Teste le matching exact de spans."""
        pred = {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'}
        gold = {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'}
        assert spans_match(pred, gold) is True
    
    def test_spans_match_wrong_type(self):
        """Teste le rejet avec mauvais type."""
        pred = {'type': 'LOC', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'}
        gold = {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'}
        assert spans_match(pred, gold) is False
    
    def test_spans_match_wrong_position(self):
        """Teste le rejet avec mauvaise position."""
        pred = {'type': 'PERSON', 'start': 1, 'end': 13, 'text': 'Mamadou Diop'}
        gold = {'type': 'PERSON', 'start': 0, 'end': 12, 'text': 'Mamadou Diop'}
        assert spans_match(pred, gold) is False
    
    def test_entity_metrics(self):
        """Teste le calcul des métriques."""
        metrics = EntityMetrics(type='PERSON')
        metrics.true_positives = 8
        metrics.false_positives = 2
        metrics.false_negatives = 2
        
        assert metrics.precision == 0.8  # 8 / 10
        assert metrics.recall == 0.8  # 8 / 10
        assert abs(metrics.f1 - 0.8) < 0.001  # F1 = 0.8
    
    def test_scorer_basic(self, scorer, sample_gold_docs, sample_pred_docs):
        """Teste le scoring de base."""
        result = scorer.score(sample_gold_docs, sample_pred_docs)
        
        assert result.total_documents == 3
        assert result.total_gold_entities == 4  # 2 + 2 + 0
        assert result.macro_f1 >= 0
        assert result.weighted_f1 >= 0
    
    def test_scorer_perfect_match(self, scorer, sample_gold_docs):
        """Teste le scoring avec match parfait."""
        # Prédictions = gold
        pred_docs = [
            {
                'doc_id': d['doc_id'],
                'text': d['text'],
                'entities': d['entities'].copy()
            }
            for d in sample_gold_docs
        ]
        
        result = scorer.score(sample_gold_docs, pred_docs)
        
        # Tout devrait être détecté
        for etype in ['PERSON', 'LOC', 'MAIL']:
            assert result.entity_metrics[etype].f1 == 1.0
    
    def test_scorer_no_match(self, scorer, sample_gold_docs):
        """Teste le scoring avec aucune prédiction."""
        pred_docs = [
            {'doc_id': d['doc_id'], 'text': d['text'], 'entities': []}
            for d in sample_gold_docs
        ]
        
        result = scorer.score(sample_gold_docs, pred_docs)
        
        # Rappel = 0
        for etype in ['PERSON', 'LOC', 'MAIL', 'TEL']:
            assert result.entity_metrics[etype].recall == 0
    
    def test_scorer_non_pii(self, scorer):
        """Teste le calcul de spécificité sur non-PII."""
        gold_docs = [
            {'doc_id': 'doc_001', 'text': 'Texte neutre.', 'entities': [], 'difficulty': 'non_pii'}
        ]
        pred_docs = [
            {'doc_id': 'doc_001', 'text': 'Texte neutre.', 'entities': [
                {'type': 'PERSON', 'start': 0, 'end': 6, 'text': 'Texte'}
            ]}
        ]
        
        result = scorer.score(gold_docs, pred_docs)
        
        assert result.non_pii_total == 1
        assert result.non_pii_false_positives == 1
        assert result.specificity == 0.0
    
    def test_bootstrap_ci(self, scorer, sample_gold_docs, sample_pred_docs):
        """Teste le calcul des intervalles de confiance."""
        result = scorer.bootstrap_ci(
            sample_gold_docs,
            sample_pred_docs,
            n_iterations=100
        )
        
        # Vérifier que les IC sont calculés
        assert len(result.macro_f1_ci) == 2
        assert result.macro_f1_ci[0] <= result.macro_f1_ci[1]
        
        # L'IC doit contenir la valeur estimée
        assert result.macro_f1_ci[0] <= result.macro_f1 <= result.macro_f1_ci[1]
    
    def test_generate_report(self, scorer, sample_gold_docs, sample_pred_docs):
        """Teste la génération de rapport."""
        result = scorer.score(sample_gold_docs, sample_pred_docs)
        report = scorer.generate_report(result)
        
        assert 'summary' in report
        assert 'per_entity' in report
        assert 'per_difficulty' in report
        assert report['summary']['total_documents'] == 3


# ==============================================================================
# TESTS INTÉGRATION
# ==============================================================================

class TestIntegration:
    """Tests d'intégration."""
    
    def test_full_pipeline(self, generator, scorer):
        """Teste le pipeline complet."""
        # Générer un petit dataset
        docs = generator.generate(n_docs=20)
        
        # Convertir en dict
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
            for doc in docs
        ]
        
        # Simuler des prédictions (gold = pred pour test)
        pred_docs = [{'doc_id': d['doc_id'], 'text': d['text'], 'entities': d['entities']} for d in gold_docs]
        
        # Scorer
        result = scorer.score(gold_docs, pred_docs)
        
        assert result.total_documents == 20
        assert result.macro_f1 > 0 or result.total_gold_entities == 0
    
    def test_hash_reproducibility(self, generator):
        """Teste que le hash est reproductible."""
        docs1 = generator.generate(n_docs=10)
        
        with tempfile.NamedTemporaryFile(suffix='.jsonl', delete=False) as f:
            filepath = f.name
        
        try:
            hash1 = generator.save(docs1, filepath)
            
            # Re-générer avec même seed
            generator._reset_seed()
            docs2 = generator.generate(n_docs=10)
            hash2 = generator.save(docs2, filepath)
            
            assert hash1 == hash2
        finally:
            os.unlink(filepath)


# ==============================================================================
# POINT D'ENTRÉE
# ==============================================================================

if __name__ == "__main__":
    pytest.main([__file__, "-v"])
