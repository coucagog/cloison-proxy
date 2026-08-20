"""Oracle de référence Presidio : analyzeur FR + regex CNI + gazetteers.

Chargement LAZY (au premier appel) : si presidio_analyzer ou le modèle spaCy
est absent, le wrapper bascule en mode « regex + gazetteers » (déterministe,
sans réseau) et le service continue de tourner — dégradation gracieuse.

Le wrapper ne fait que DÉTECTER : aucun span n'est persisté ni journalisé
(logging = compteurs + avertissements, jamais de texte).
"""

from __future__ import annotations

import logging
import re
import threading
from typing import Mapping, Sequence

from .config import Config
from .spans import (
    Policy,
    Span,
    SpanType,
    insensitive_pattern,
)

logger = logging.getLogger(__name__)

# Regex CNI (numéro de carte nationale d'identité, 12 chiffres).
_CNI_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"(?<!\d)\d{12}(?!\d)"),
    re.compile(r"(?<!\d)\d{2}[ ]\d{4}[ ]\d{4}[ ]\d{2}(?!\d)"),
)

# Correspondance entité Presidio -> SpanType canonique.
_ENTITY_MAP: dict[str, SpanType] = {
    "PERSON": SpanType.PERSON,
    "PERSON_NAME": SpanType.PERSON,
    "LOCATION": SpanType.LOC,
    "GPE": SpanType.LOC,
    "ORGANIZATION": SpanType.ORG,
    "DATE_TIME": SpanType.DATE,
    "DATE": SpanType.DATE,
    "AGE": SpanType.AGE,
    "CNI": SpanType.ID,
    "ID": SpanType.ID,
}


class PresidioOracle:
    """Oracle de référence (baseline STACK-1) + backend possible.

    `detect()` combine toujours (a) l'analyzer Presidio s'il est chargé et
    (b) les regex CNI + gazetteers, déterministes et sans réseau.
    """

    name = "presidio"

    def __init__(self, config: Config) -> None:
        self._config = config
        self._lock = threading.Lock()
        self._analyzer = None            # type: ignore[assignment]
        self._load_attempted = False
        self._compiled_gazetteers = self._compile_gazetteers(config.presidio.gazetteers)

    # -- état ----------------------------------------------------------------
    def available(self) -> bool:
        """Toujours vrai : le mode regex + gazetteers est opérationnel même
        sans Presidio (dégradation gracieuse)."""
        return True

    def presidio_loaded(self) -> bool:
        return self._analyzer is not None

    def preload(self) -> None:
        self._ensure_loaded()

    # -- chargement lazy -----------------------------------------------------
    def _ensure_loaded(self) -> None:
        if self._load_attempted:
            return
        with self._lock:
            if self._load_attempted:
                return
            self._load_attempted = True
            if not self._config.presidio.enabled:
                logger.info("presidio: désactivé par configuration — regex+gazetteers seuls")
                return
            try:
                from presidio_analyzer import AnalyzerEngine, Pattern, PatternRecognizer
                from presidio_analyzer.nlp_engine import NlpEngineProvider
            except Exception as exc:  # ImportError, version incompatible, ...
                logger.warning("presidio: import indisponible (%s) — regex+gazetteers", exc)
                return
            try:
                provider = NlpEngineProvider(
                    nlp_configuration={
                        "nlp_engine_name": "spacy",
                        "models": [
                            {"lang_code": lang, "model_name": self._spacy_model(lang)}
                            for lang in self._config.presidio.languages
                        ],
                    }
                )
                nlp_engine = provider.create_engine()
                analyzer = AnalyzerEngine(
                    nlp_engine=nlp_engine,
                    supported_languages=list(self._config.presidio.languages),
                )
                if self._config.presidio.enable_cni:
                    analyzer.registry.add_recognizer(self._cni_recognizer(Pattern, PatternRecognizer))
                for entity, names in self._config.presidio.gazetteers.items():
                    analyzer.registry.add_recognizer(
                        self._gazetteer_recognizer(entity, names, Pattern, PatternRecognizer)
                    )
                self._analyzer = analyzer
                logger.info(
                    "presidio: oracle chargé (langues=%s, spacy=%s)",
                    ",".join(self._config.presidio.languages),
                    self._config.spacy_size,
                )
            except Exception as exc:
                logger.warning("presidio: chargement impossible (%s) — regex+gazetteers", exc)

    # -- reconnaisseurs custom ------------------------------------------------
    def _cni_recognizer(self, pattern_cls, recognizer_cls):  # type: ignore[no-untyped-def]
        patterns = [
            pattern_cls(name="cni_12_digits", regex=p.pattern, score=self._config.presidio.cni_score)
            for p in _CNI_PATTERNS
        ]
        return recognizer_cls(supported_entity="CNI", patterns=patterns, supported_language="fr")

    def _gazetteer_recognizer(self, entity, names, pattern_cls, recognizer_cls):  # type: ignore[no-untyped-def]
        patterns = [
            pattern_cls(
                name=f"gaz_{entity}_{i}",
                regex=p.pattern,
                score=self._config.presidio.gazetteer_score,
            )
            for i, p in enumerate(self._compiled_gazetteers.get(entity, []))
        ]
        return recognizer_cls(supported_entity=entity, patterns=patterns, supported_language="fr")

    @staticmethod
    def _compile_gazetteers(gazetteers: Mapping[str, Sequence[str]]) -> dict[str, list[re.Pattern[str]]]:
        """Regex mot entier, insensible casse + diacritiques (cf. spans.insensitive_pattern)."""
        compiled: dict[str, list[re.Pattern[str]]] = {}
        for entity, names in gazetteers.items():
            compiled[entity] = [
                re.compile(
                    r"(?<![\w-])" + insensitive_pattern(name) + r"(?![\w-])",
                    re.IGNORECASE | re.UNICODE,
                )
                for name in names
            ]
        return compiled

    def _spacy_model(self, lang: str) -> str:
        if lang == "fr":
            return f"fr_core_news_{self._config.spacy_size}"
        return "en_core_web_sm" if self._config.spacy_size == "sm" else "en_core_web_lg"

    # -- détection ------------------------------------------------------------
    def detect(self, text: str, locale: str = "fr", policy: Policy | None = None) -> list[Span]:
        """Détecte PERSON/LOC/ORG/DATE/AGE/ID via Presidio (si chargé) + regex.

        Renvoie des spans NON filtrés par seuil de score ; le filtrage par type
        demandé (policy.types) est appliqué ici (optimisation, pas décision).
        """
        if not text:
            return []
        self._ensure_loaded()
        spans: list[Span] = []
        if self._analyzer is not None:
            try:
                lang = self._language_for(locale)
                for result in self._analyzer.analyze(text=text, language=lang):
                    t = _ENTITY_MAP.get(str(result.entity_type), SpanType.UNKNOWN)
                    if t is SpanType.UNKNOWN:
                        continue
                    spans.append(
                        Span(
                            start=int(result.start),
                            end=int(result.end),
                            type=t,
                            score=float(result.score),
                            source=self.name,
                        )
                    )
            except Exception as exc:
                logger.warning("presidio: analyse échouée (%s) — regex+gazetteers seuls", exc)
        spans.extend(self._detect_regex(text))
        spans = self._dedupe(spans)
        return self._filter_types(spans, policy)

    def _detect_regex(self, text: str) -> list[Span]:
        """CNI + gazetteers : déterministe, sans réseau, toujours actif."""
        spans: list[Span] = []
        cfg = self._config.presidio
        if cfg.enable_cni:
            for pattern in _CNI_PATTERNS:
                for m in pattern.finditer(text):
                    spans.append(
                        Span(m.start(), m.end(), SpanType.ID, cfg.cni_score, source="presidio:regex")
                    )
        for entity, patterns in self._compiled_gazetteers.items():
            t = SpanType.parse(entity)
            if t is SpanType.UNKNOWN:
                continue
            for pattern in patterns:
                for m in pattern.finditer(text):
                    spans.append(
                        Span(m.start(), m.end(), t, cfg.gazetteer_score, source="presidio:gazetteer")
                    )
        return spans

    def _language_for(self, locale: str) -> str:
        lang = (locale or "fr").split("-")[0].lower()
        if lang in self._config.presidio.languages:
            return lang
        return "fr"  # l'oracle est francophone par défaut ; langues africaines -> fr

    @staticmethod
    def _dedupe(spans: list[Span]) -> list[Span]:
        """Garde le meilleur score par (start, end, type) ; tri stable par offsets."""
        best: dict[tuple[int, int, SpanType], Span] = {}
        for s in spans:
            if s.type is SpanType.UNKNOWN:
                continue
            key = (s.start, s.end, s.type)
            if key not in best or s.score > best[key].score:
                best[key] = s
        return sorted(best.values(), key=lambda s: (s.start, s.end))

    @staticmethod
    def _filter_types(spans: list[Span], policy: Policy | None) -> list[Span]:
        if policy is None or not policy.types:
            return spans
        return [s for s in spans if s.type in policy.types]
