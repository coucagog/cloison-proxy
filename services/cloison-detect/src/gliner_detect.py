"""Détecteur GLiNER zéro-shot (labels custom PERSON / LOC / ORG).

Chargement LAZY au premier appel, dégradation gracieuse : si le paquet gliner
ou le modèle est absent (hors-ligne), `detect()` renvoie [] et le service
continue. Après un crash (OOM, modèle corrompu), le modèle est mis en
quarantaine : pas de rechargement pendant CLOISON_QUARANTINE_SECONDS.

Le détecteur ne fait que DÉTECTER ; le filtrage final appartient à la fusion.
"""

from __future__ import annotations

import logging
import threading
import time

from .config import Config
from .spans import Policy, Span, SpanType

logger = logging.getLogger(__name__)


class GlinerDetector:
    """NER zéro-shot GLiNER (rappel PERSON/LOC pour les types custom)."""

    name = "gliner"

    def __init__(self, config: Config) -> None:
        self._config = config
        self._lock = threading.Lock()
        self._model = None                # type: ignore[assignment]
        self._load_attempted = False
        self._quarantine_until = 0.0
        # label GLiNER (fr) -> SpanType, depuis la configuration
        self._label_to_type: dict[str, SpanType] = {}
        for type_name, labels in config.gliner.labels.items():
            st = SpanType.parse(type_name)
            for label in labels:
                self._label_to_type[label] = st

    # -- état ----------------------------------------------------------------
    def available(self) -> bool:
        if time.monotonic() < self._quarantine_until:
            return False
        return self._model is not None

    def loaded(self) -> bool:
        return self._model is not None

    def preload(self) -> None:
        self._ensure_loaded()

    # -- chargement lazy -----------------------------------------------------
    def _ensure_loaded(self) -> None:
        if self._load_attempted or self._model is not None:
            return
        with self._lock:
            if self._load_attempted or self._model is not None:
                return
            self._load_attempted = True
            if not self._config.gliner.enabled:
                logger.info("gliner: désactivé par configuration")
                return
            try:
                from gliner import GLiNER  # import lazy : paquet absent -> dégradation
            except Exception as exc:
                logger.warning("gliner: paquet indisponible (%s) — détecteur inactif", exc)
                return
            try:
                model = GLiNER.from_pretrained(self._config.gliner.model_id)
                model.eval()
                self._model = model
                logger.info("gliner: modèle chargé (%s)", self._config.gliner.model_id)
            except Exception as exc:
                logger.warning("gliner: chargement impossible (%s) — détecteur inactif", exc)
                self._quarantine_until = time.monotonic() + self._config.quarantine_seconds

    # -- détection ------------------------------------------------------------
    def detect(self, text: str, locale: str = "fr", policy: Policy | None = None) -> list[Span]:
        """Détecte PERSON/LOC/ORG en zéro-shot. [] si le modèle est indisponible."""
        if not text:
            return []
        self._ensure_loaded()
        if self._model is None:
            return []
        labels = self._labels_for(policy)
        if not labels:
            return []
        try:
            # Seuil interne de candidature : câblé sur la config (défaut 0.45).
            # Le 0.05 codé en dur inondait la fusion de candidats bruités
            # (ORG à 0.064, Wolof « Nanga/Maa/Jërëjëf » → PERSON) — contribution
            # directe aux faux positifs (spécificité 27 % au benchmark).
            entities = self._model.predict_entities(
                text, labels, threshold=self._config.gliner.threshold
            )
        except Exception as exc:
            logger.warning("gliner: prédiction échouée (%s) — spans ignorés", exc)
            self._quarantine_until = time.monotonic() + self._config.quarantine_seconds
            return []
        spans: list[Span] = []
        for entity in entities:
            t = self._label_to_type.get(str(entity.get("label", "")), SpanType.UNKNOWN)
            if t is SpanType.UNKNOWN:
                continue
            start, end = int(entity["start"]), int(entity["end"])
            if not (0 <= start < end):
                continue
            spans.append(
                Span(start=start, end=end, type=t, score=float(entity["score"]), source=self.name)
            )
        return spans

    def _labels_for(self, policy: Policy | None) -> list[str]:
        """Labels demandés par la politique (types vides = tous)."""
        wanted = policy.types if policy is not None else frozenset()
        labels: list[str] = []
        for type_name, group in self._config.gliner.labels.items():
            st = SpanType.parse(type_name)
            if not wanted or st in wanted:
                labels.extend(group)
        return labels
