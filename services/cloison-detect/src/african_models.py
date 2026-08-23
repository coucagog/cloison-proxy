"""Détecteur NER ouest-africain : SERENGETI / AfroXLMR / MasakhaNER 2.0 (F-43).

Chargement LAZY au premier appel (transformers, AutoModelForTokenClassification
+ AutoTokenizer), dégradation gracieuse : paquet transformers absent, modèle
non téléchargeable (hors-ligne) ou prédiction en échec -> `detect()` renvoie
[] et le service continue. Après un crash (OOM, modèle corrompu), le modèle
est mis en quarantaine (même pattern que GlinerDetector).

Modèles supportés (config `african.model_name`) :
  - serengeti  : SERENGETI (UBC-NLP) ; le checkpoint de base est un LM, un
                 fine-tune NER (ex. MasakhaNER) doit être configuré via
                 `african.model_ids` pour l'activer ;
  - afroxlmr   : AfroXLMR fine-tuné NER (masakhane, MasakhaNER 1.0+2.0) ;
  - masakha    : alias MasakhaNER 2.0 (Davlan).

Le détecteur ne fait que DÉTECTER ; le filtrage final appartient à la fusion.
"""

from __future__ import annotations

import logging
import os
import threading
import time

from .config import Config
from .spans import Policy, Span, SpanType

logger = logging.getLogger(__name__)

# transformers est OPTIONNEL : s'il est absent, le détecteur se dégrade en
# stub (jamais disponible, detect() -> []), le pipeline continue.
try:  # pragma: no cover - dépend de l'environnement d'exécution
    from transformers import AutoModelForTokenClassification, AutoTokenizer
except ImportError:  # pragma: no cover
    AutoModelForTokenClassification = None  # type: ignore[assignment]
    AutoTokenizer = None  # type: ignore[assignment]


# Correspondance labels NER (variantes BIO et plain, fr/en) -> type canonique.
_LABEL_TYPE_MAP: dict[str, SpanType] = {
    "PER": SpanType.PERSON,
    "PERSON": SpanType.PERSON,
    "LOC": SpanType.LOC,
    "LOCATION": SpanType.LOC,
    "GPE": SpanType.LOC,
    "ORG": SpanType.ORG,
    "ORGANIZATION": SpanType.ORG,
    "ORGANISATION": SpanType.ORG,
}

# Noms de modèles acceptés par `african.model_name` (alias masakha inclus).
SUPPORTED_MODELS: tuple[str, ...] = ("serengeti", "afroxlmr", "masakha")


class AfricanModelDetector:
    """NER ouest-africain pluggable (SERENGETI / AfroXLMR / MasakhaNER 2.0)."""

    #: noms de modèles acceptés par `african.model_name`
    supported_models: tuple[str, ...] = SUPPORTED_MODELS

    def __init__(self, config: Config, model_name: str | None = None) -> None:
        self._config = config
        self._model_name = (model_name or config.african.model_name).strip().lower()
        self._lock = threading.Lock()
        self._model = None            # type: ignore[assignment]
        self._tokenizer = None        # type: ignore[assignment]
        self._load_attempted = False
        self._quarantine_until = 0.0
        # source pour la fusion (poids d'ensemble) : serengeti | afro
        self.name = "serengeti" if self._model_name == "serengeti" else "afro"

    # -- état ----------------------------------------------------------------
    def available(self) -> bool:
        if time.monotonic() < self._quarantine_until:
            return False
        return self._model is not None

    def loaded(self) -> bool:
        return self._model is not None

    def preload(self) -> None:
        self._ensure_loaded()

    def status(self) -> dict[str, object]:
        """État exposé par model_status() (/models, healthz, 503 de l'API)."""
        return {
            "loaded": self.loaded(),
            "available": self.available(),
            "model": self._model_name,
            "model_id": self._model_id() or None,
        }

    # -- chargement lazy -----------------------------------------------------
    def _ensure_loaded(self) -> None:
        if self._load_attempted or self._model is not None:
            return
        with self._lock:
            if self._load_attempted or self._model is not None:
                return
            self._load_attempted = True
            if not self._config.african.enabled:
                logger.info("african: désactivé par configuration")
                return
            if AutoModelForTokenClassification is None or AutoTokenizer is None:
                logger.warning("african: transformers indisponible — détecteur inactif (stub)")
                return
            if self._model_name not in self.supported_models:
                logger.warning("african: modèle inconnu (%r) — détecteur inactif", self._model_name)
                return
            model_id = self._model_id()
            try:
                kwargs: dict[str, object] = {}
                if self._offline():
                    kwargs["local_files_only"] = True  # hors-ligne : cache local uniquement
                tokenizer = AutoTokenizer.from_pretrained(model_id, **kwargs)
                model = AutoModelForTokenClassification.from_pretrained(model_id, **kwargs)
                model.eval()
                self._tokenizer = tokenizer
                self._model = model
                logger.info("african: modèle chargé (%s, %s)", self._model_name, model_id)
            except Exception as exc:
                logger.warning("african: chargement impossible (%s) — détecteur inactif", exc)
                self._quarantine_until = time.monotonic() + self._config.quarantine_seconds

    # -- détection ------------------------------------------------------------
    def detect(self, text: str, locale: str = "fr", policy: Policy | None = None) -> list[Span]:
        """Détecte PERSON/LOC/ORG. [] si le modèle est indisponible."""
        if not text:
            return []
        self._ensure_loaded()
        if self._model is None or self._tokenizer is None:
            return []
        try:
            import torch  # présent dès que transformers l'est

            encoded = self._tokenizer(
                text,
                return_offsets_mapping=True,
                return_tensors="pt",
                truncation=True,
                max_length=512,
            )
            # `offset_mapping` n'est PAS un argument de `forward()` (signature
            # sans **kwargs) : le passer au modèle levait une TypeError — le
            # détecteur africain renvoyait [] EN SILENCE avec transformers
            # 4.46.3 épinglé (correction DEPLOY-6 ; le venv non-pinné du
            # STACK-8 tolérait le kwarg — d'où le verdict GO mesuré).
            offsets = encoded.pop("offset_mapping")
            with torch.no_grad():
                logits = self._model(**encoded).logits
            pred_ids = logits.argmax(dim=-1)[0].tolist()
            probs = torch.softmax(logits, dim=-1)
            spans = self._align_spans(offsets, pred_ids, probs)
        except Exception as exc:
            logger.warning("african: prédiction échouée (%s) — spans ignorés", exc)
            self._quarantine_until = time.monotonic() + self._config.quarantine_seconds
            return []
        return self._filter_types(spans, policy)

    def _align_spans(
        self,
        offsets,
        pred_ids: list[int],
        probs,
    ) -> list[Span]:
        """Aligne tokens -> offsets caractères ; regroupe les tokens contigus
        de même type en spans (gère les préfixes BIO)."""
        id2label: dict[int, str] = getattr(self._model.config, "id2label", {}) or {}
        offsets = offsets[0].tolist()

        spans: list[Span] = []
        current_type: SpanType | None = None
        current_start = 0
        current_end = 0
        token_probs: list[float] = []

        def flush() -> None:
            nonlocal current_type, token_probs
            if current_type is None or not token_probs:
                return
            span = self._make_span(current_type, current_start, current_end, token_probs)
            if span is not None:
                spans.append(span)
            current_type = None
            token_probs = []

        for i, (tok_start, tok_end) in enumerate(offsets):
            if tok_start is None or tok_end is None or tok_start >= tok_end:
                flush()  # token spécial / padding
                continue
            t = self._type_of_label(str(id2label.get(int(pred_ids[i]), "O")))
            if t is None:
                flush()
                continue
            prob = float(probs[0, i, int(pred_ids[i])].item())
            if t == current_type:
                current_end = tok_end
                token_probs.append(prob)
            else:
                flush()
                current_type = t
                current_start = tok_start
                current_end = tok_end
                token_probs = [prob]
        flush()
        return spans

    def _make_span(
        self, stype: SpanType, start: int, end: int, probs: list[float]
    ) -> Span | None:
        """Score = probabilité moyenne des tokens ; seuil interne bas (la fusion filtre)."""
        score = sum(probs) / len(probs) if probs else 0.0
        if score < self._config.african.threshold:
            return None
        return Span(start=start, end=end, type=stype, score=round(score, 4), source=self.name)

    @staticmethod
    def _type_of_label(label: str) -> SpanType | None:
        """Label NER -> SpanType canonique (BIO : B-PER/I-PER, plain : PER...)."""
        raw = str(label).strip()
        if not raw or raw in ("O", "OUT"):
            return None
        core = raw
        if core[:1] in ("B", "I", "E", "S") and "-" in core:
            core = core.split("-", 1)[1]
        return _LABEL_TYPE_MAP.get(core.upper())

    @staticmethod
    def _filter_types(spans: list[Span], policy: Policy | None) -> list[Span]:
        """Filtrage par types demandés (optimisation ; la fusion refiltre)."""
        if policy is None or not policy.types:
            return spans
        return [s for s in spans if s.type in policy.types]

    # -- utilitaires ----------------------------------------------------------
    def _model_id(self) -> str:
        """Identifiant Hugging Face du modèle configuré ('' si inconnu)."""
        return str(self._config.african.model_ids.get(self._model_name, "") or "")

    def _offline(self) -> bool:
        """Hors-ligne : config.offline OU env CLOISON_OFFLINE (aucun téléchargement)."""
        raw = os.environ.get("CLOISON_OFFLINE", "")
        return self._config.offline or raw.strip().lower() in ("1", "true", "yes", "on")
