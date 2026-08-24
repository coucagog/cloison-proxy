#!/usr/bin/env python3
"""
CLOISON STACK-1 Benchmark - Baseline Presidio

Configuration forte de Presidio pour la détection PII sénégalaise:
- AnalyzerEngine avec langue française
- SpacyRecognizer (fr_core_news_md)
- EmailRecognizer, PhoneRecognizer
- PatternRecognizer custom pour CNI sénégalaise (regex + validation Luhn)
- Gazetteers PERSON/LOC via PatternRecognizer

Seuil de détection: 0.0 (rappel maximal)
AUCUN réentraînement sur les données du benchmark.
"""

import re
from typing import List, Dict, Optional, Tuple

# Imports Presidio
try:
    from presidio_analyzer import (
        AnalyzerEngine,
        RecognizerRegistry,
        PatternRecognizer,
        Pattern,
        EntityRecognizer,
        RecognizerResult
    )
    from presidio_analyzer.nlp_engine import NlpEngineProvider
    from presidio_analyzer.predefined_recognizers import (
        EmailRecognizer,
        PhoneRecognizer,
        SpacyRecognizer
    )
    PRESIDIO_AVAILABLE = True
except ImportError:
    PRESIDIO_AVAILABLE = False
    print("WARNING: presidio-analyzer not installed. Install with: pip install presidio-analyzer")


# ==============================================================================
# VALIDATEUR LUHN POUR CNI
# ==============================================================================

from generator import validate_cni_luhn  # source unique

# ==============================================================================
# RECOGNIZER CUSTOM POUR CNI SÉNÉGALAISE
# ==============================================================================

class CNISenegalRecognizer(PatternRecognizer):
    """
    Recognizer custom pour la CNI sénégalaise.
    
    Format: 13 chiffres commençant par 1
    Validation: Algorithme Luhn
    Patterns: avec et sans espaces
    """
    
    # Patterns regex pour CNI
    PATTERNS = [
        # Format compact: 13 chiffres commençant par 1
        Pattern(
            name="cni_compact",
            regex=r"1\d{12}",
            score=0.7
        ),
        # Format avec espaces: 13 chiffres (NIN sénégalais biométrique),
        # groupes 3-3-4-3 (ex: 114 979 7316 811).
        Pattern(
            name="cni_formatted",
            regex=r"1\d{2}\s\d{3}\s\d{4}\s\d{3}",
            score=0.8
        ),
    ]
    
    # Mots de contexte pour améliorer la détection
    CONTEXT = [
        "cni", "carte", "identité", "numéro", "nina",
        "carte d'identité", "pièce", "document",
        "national", "identification"
    ]
    
    def __init__(self):
        super().__init__(
            supported_entity="CNI",
            supported_language="fr",
            patterns=self.PATTERNS,
            context=self.CONTEXT
        )
    
    def validate_result(self, pattern_text: str) -> bool:
        """
        Valide le résultat avec l'algorithme Luhn.
        
        Cette méthode est appelée après le match du pattern
        pour filtrer les faux positifs.
        """
        return validate_cni_luhn(pattern_text)
    
    def enhance_score(self, text: str, pattern_match: str) -> float:
        """
        Augmente le score si des mots de contexte sont présents.
        """
        base_score = 0.7
        
        # Vérifier la présence de mots de contexte
        text_lower = text.lower()
        context_boost = 0
        
        for context_word in self.CONTEXT:
            if context_word in text_lower:
                context_boost += 0.05
        
        # Score final (max 1.0)
        return min(base_score + context_boost, 1.0)


# ==============================================================================
# RECOGNIZER POUR TÉLÉPHONE SÉNÉGALAIS
# ==============================================================================

class SenegalPhoneRecognizer(PatternRecognizer):
    """
    Recognizer étendu pour les téléphones sénégalais.
    
    Formats supportés:
    - +221 XXXXXXXXX
    - 00221 XXXXXXXXX
    - 7X XXX XX XX (formats locaux)
    - (+221) 7X XXX XX XX
    
    Préfixes opérateurs: 70, 71, 75, 76, 77, 78 (mobiles) ;
    fixes : 30, 32, 33, 36 (zone 8/9) — N3+.
    """
    
    PATTERNS = [
        # Format international complet (mobile)
        Pattern(
            name="tel_international",
            regex=r"\+221(?:70|71|75|76|77|78)\d{7}",
            score=0.9
        ),
        # Format international complet (fixe : 30/32/33/36 + zone 8/9)
        Pattern(
            name="tel_international_fixe",
            regex=r"\+221(?:30|32|33|36)[89]\d{6}",
            score=0.9
        ),
        # Format international avec 00
        Pattern(
            name="tel_international_00",
            regex=r"00221(?:70|71|75|76|77|78)\d{7}",
            score=0.85
        ),
        # Format local mobile 9 chiffres
        Pattern(
            name="tel_local",
            regex=r"(?:70|71|75|76|77|78)\d{7}",
            score=0.6
        ),
        # Format local fixe 8 chiffres (30/32/33/36 + zone 8/9)
        Pattern(
            name="tel_local_fixe",
            regex=r"(?:30|32|33|36)[89]\d{6}",
            score=0.6
        ),
        # Format formaté avec espaces
        Pattern(
            name="tel_formatted",
            regex=r"\+221\s?(?:70|71|75|76|77|78|30|32|33|36)\s?\d{3}\s?\d{2}\s?\d{2}",
            score=0.85
        ),
        # Format avec parenthèses
        Pattern(
            name="tel_parentheses",
            regex=r"\(\+221\)\s?(?:70|71|75|76|77|78|30|32|33|36)\s?\d{3}\s?\d{2}\s?\d{2}",
            score=0.85
        ),
    ]
    
    CONTEXT = [
        "téléphone", "tel", "mobile", "portable", "numéro",
        "contact", "appeler", "joindre", "cellulaire"
    ]
    
    def __init__(self):
        super().__init__(
            supported_entity="TEL",
            supported_language="fr",
            patterns=self.PATTERNS,
            context=self.CONTEXT
        )


# ==============================================================================
# RECOGNIZER CONTEXTUEL — PASSEPORT / PERMIS / MATRICULE (N3+)
# ==============================================================================

class SenegalContextualIDRecognizer(PatternRecognizer):
    """
    Identifiants contextuels sénégalais : numéro de passeport, permis de
    conduire, matricule de fonctionnaire de l'État / assuré IPRES (actifs et
    retraités).

    La détection est CONTEXTUELLE (mot-clé + numéro) pour limiter les faux
    positifs ; les formats exacts ne sont pas documentés publiquement —
    structure observée, à confirmer (charte §11 : périmètre honnête).
    HORS grille GO (le benchmark reste figé sur PERSON/LOC/CNI/MAIL/TEL) :
    ces entités mesurent la couverture produit, sans critère.
    """

    PATTERNS = [
        # Passeport : 1-2 lettres + 7-8 chiffres (CEDEAO/ICAO observé)
        Pattern(
            name="id_passeport",
            regex=r"(?i)(?:passeport|passport)\s*(?:n[°o]\s*)?[:#]?\s*([A-Z]{1,2}[0-9]{7,8})",
            score=0.85
        ),
        # Permis de conduire : 7-10 chiffres
        Pattern(
            name="id_permis",
            regex=r"(?i)(?:permis\s+de\s+conduire|permis|driver\s+license|licence)\s*(?:n[°o]\s*)?[:#]?\s*([0-9]{7,10})",
            score=0.85
        ),
        # Matricule État/IPRES : 8-11 chiffres
        Pattern(
            name="id_matricule",
            regex=r"(?i)(?:matricule|ipres|immatriculation)\s*(?:n[°o]\s*)?[:#]?\s*([0-9]{8,11})",
            score=0.85
        ),
    ]

    CONTEXT = [
        "passeport", "passport", "permis", "conduire", "driver",
        "matricule", "ipres", "immatriculation", "fonctionnaire", "retraité"
    ]

    def __init__(self):
        super().__init__(
            supported_entity="ID_CONTEXTUEL",
            supported_language="fr",
            patterns=self.PATTERNS,
            context=self.CONTEXT
        )


# ==============================================================================
# GAZETTEERS PERSON ET LOC
# ==============================================================================

# Ces listes sont importées depuis generator.py
# mais définies ici aussi pour l'autonomie du module

from generator import (
    PRENOMS_MASCULIN,
    PRENOMS_FEMININ,
    PATRONYMES,
    REGIONS,
    VILLES,
    QUARTIERS_DAKAR,
)

# Source unique : les listes du générateur, pas de duplication.
PRENOMS_SENEGAL = PRENOMS_MASCULIN + PRENOMS_FEMININ
PATRONYMES_SENEGAL = PATRONYMES
LOCATIONS_SENEGAL = REGIONS + VILLES + QUARTIERS_DAKAR


def create_person_gazetteer_recognizer() -> PatternRecognizer:
    """
    Crée un recognizer basé sur une liste de noms sénégalais.
    
    Utilise les prénoms et patronymes comme deny-list
    pour améliorer la détection des entités PERSON.
    """
    # Combiner prénoms et patronymes
    all_names = list(set(PRENOMS_SENEGAL + PATRONYMES_SENEGAL))
    
    return PatternRecognizer(
        supported_entity="PERSON",
        supported_language="fr",
        deny_list=all_names,
        deny_list_score=0.4  # Score conservateur pour les matches exacts
    )


def create_loc_gazetteer_recognizer() -> PatternRecognizer:
    """
    Crée un recognizer basé sur une liste de lieux sénégalais.
    
    Utilise les toponymes comme deny-list
    pour améliorer la détection des entités LOC.
    """
    all_locations = list(set(LOCATIONS_SENEGAL))
    
    return PatternRecognizer(
        supported_entity="LOC",
        supported_language="fr",
        deny_list=all_locations,
        deny_list_score=0.5  # Score conservateur pour les matches exacts
    )


# ==============================================================================
# CONFIGURATION DE LA BASELINE
# ==============================================================================

def create_baseline_analyzer(
    spacy_model: str = "fr_core_news_md",
    score_threshold: float = 0.0
) -> Optional['AnalyzerEngine']:
    """
    Crée et configure l'AnalyzerEngine Presidio.
    
    Configuration:
    - Langue: français
    - SpacyRecognizer avec modèle français
    - EmailRecognizer
    - PhoneRecognizer + SenegalPhoneRecognizer
    - CNISenegalRecognizer
    - Gazetteers PERSON/LOC
    
    Args:
        spacy_model: Modèle spaCy à utiliser (défaut: fr_core_news_md)
        score_threshold: Seuil de détection (défaut: 0.0 pour rappel max)
        
    Returns:
        AnalyzerEngine configuré, ou None si Presidio non disponible
    """
    if not PRESIDIO_AVAILABLE:
        return None
    
    # Créer le registry — supported_languages DOIT être passé au constructeur
    # (sinon il défaut sur ['en'] et AnalyzerEngine rejette l'engine fr).
    registry = RecognizerRegistry(supported_languages=['fr'])
    
    # Charger les recognizers par défaut
    registry.load_predefined_recognizers(languages=['fr'])
    
    # Moteur NLP français : NlpEngineProvider charge fr_core_news_md.
    # SpacyRecognizer ne prend PAS de paramètre model_name — le modèle est
    # fourni par le nlp_engine passé à AnalyzerEngine.
    try:
        nlp_configuration = {
            "nlp_engine_name": "spacy",
            "models": [{"lang_code": "fr", "model_name": spacy_model}],
        }
        nlp_engine = NlpEngineProvider(nlp_configuration=nlp_configuration).create_engine()
        spacy_recognizer = SpacyRecognizer(
            supported_language='fr',
            supported_entities=['PERSON', 'LOC', 'ORG']
        )
        registry.add_recognizer(spacy_recognizer)
    except Exception as e:
        print(f"WARNING: Could not load spacy model {spacy_model}: {e}")
        nlp_engine = None
    
    # Ajouter EmailRecognizer
    email_recognizer = EmailRecognizer(supported_language='fr')
    registry.add_recognizer(email_recognizer)
    
    # Ajouter les recognizers téléphone sénégalais
    phone_recognizer = PhoneRecognizer(supported_language='fr')
    registry.add_recognizer(phone_recognizer)
    
    senegal_phone_recognizer = SenegalPhoneRecognizer()
    registry.add_recognizer(senegal_phone_recognizer)
    
    # Ajouter le recognizer CNI
    cni_recognizer = CNISenegalRecognizer()
    registry.add_recognizer(cni_recognizer)

    # N3+ : identifiants contextuels sénégalais (passeport, permis de
    # conduire, matricules État/IPRES) — HORS grille (non scorés par le
    # benchmark GO, qui reste figé sur PERSON/LOC/CNI/MAIL/TEL) mais
    # présents dans le jeu pour mesurer la couverture produit.
    contextual_recognizer = SenegalContextualIDRecognizer()
    registry.add_recognizer(contextual_recognizer)
    
    # Ajouter les gazetteers
    person_gazetteer = create_person_gazetteer_recognizer()
    registry.add_recognizer(person_gazetteer)
    
    loc_gazetteer = create_loc_gazetteer_recognizer()
    registry.add_recognizer(loc_gazetteer)
    
    # Créer l'analyzer
    analyzer = AnalyzerEngine(
        registry=registry,
        supported_languages=['fr'],
        default_score_threshold=score_threshold,
        nlp_engine=nlp_engine,
    )
    
    return analyzer


# Mapping des types Presidio vers les types de la grille CLOISON.
# Les entités hors grille (ORGANIZATION, URL, DATE_TIME, CRYPTO, …) sont
# écartées : le benchmark n'évalue que PERSON/LOC/CNI/MAIL/TEL.
ENTITY_TYPE_MAP = {
    "PERSON": "PERSON",
    "LOCATION": "LOC",
    "LOC": "LOC",
    "CNI": "CNI",
    "EMAIL_ADDRESS": "MAIL",
    "PHONE_NUMBER": "TEL",
    "TEL": "TEL",
}


def detect_pii(
    analyzer: 'AnalyzerEngine',
    text: str,
    language: str = 'fr',
    score_threshold: float = 0.0
) -> List[Dict]:
    """
    Détecte les entités PII dans un texte.
    
    Args:
        analyzer: AnalyzerEngine Presidio configuré
        text: Texte à analyser
        language: Langue (défaut: fr)
        score_threshold: Seuil de score minimum
        
    Returns:
        Liste d'entités détectées: [{'type': ..., 'start': ..., 'end': ..., 'text': ..., 'score': ...}]
    """
    if analyzer is None:
        return []
    
    results = analyzer.analyze(
        text=text,
        language=language,
        score_threshold=score_threshold
    )
    
    entities = []
    for result in results:
        mapped = ENTITY_TYPE_MAP.get(result.entity_type)
        if mapped is None:
            # Type hors grille (ORGANIZATION, URL, DATE_TIME, …) : ignoré.
            continue
        entities.append({
            'type': mapped,
            'start': result.start,
            'end': result.end,
            'text': text[result.start:result.end],
            'score': result.score
        })
    
    return entities


def detect_batch(
    analyzer: 'AnalyzerEngine',
    documents: List[Dict],
    text_key: str = 'text',
    language: str = 'fr',
    score_threshold: float = 0.0
) -> List[Dict]:
    """
    Détecte les PII dans un batch de documents.
    
    Args:
        analyzer: AnalyzerEngine Presidio configuré
        documents: Liste de documents (dict avec clé text_key)
        text_key: Clé du champ texte dans les documents
        language: Langue
        score_threshold: Seuil de score
        
    Returns:
        Liste de documents avec entités détectées
    """
    predictions = []
    
    for doc in documents:
        text = doc.get(text_key, '')
        entities = detect_pii(analyzer, text, language, score_threshold)
        
        predictions.append({
            'doc_id': doc.get('doc_id', ''),
            'text': text,
            'entities': entities
        })
    
    return predictions


# ==============================================================================
# POINT D'ENTRÉE
# ==============================================================================

if __name__ == "__main__":
    # Test de la baseline
    if PRESIDIO_AVAILABLE:
        print("Creating baseline analyzer...")
        analyzer = create_baseline_analyzer()
        
        if analyzer:
            # Texte de test
            test_texts = [
                "Mamadou Diop réside à Dakar. Son CNI est 175 234 5678 01 et son téléphone est +221771234567.",
                "Contactez Aminata Diallo à l'adresse aminata.diallo@email.sn ou au 76 123 45 67.",
                "Le dossier de M. Fall, né à Thiès, porte le numéro 1824567890123."
            ]
            
            for text in test_texts:
                print(f"\nText: {text}")
                entities = detect_pii(analyzer, text)
                for e in entities:
                    print(f"  - {e['type']}: '{e['text']}' (score: {e['score']:.2f})")
        else:
            print("Failed to create analyzer. Make sure spaCy model is installed.")
    else:
        print("Presidio not available. Install with: pip install presidio-analyzer spacy")
        print("Then download the French model: python -m spacy download fr_core_news_md")
