"""DetectService — pipeline complet de détection (API_DESIGN §3.3).

Orchestration (cascade de coût, décision en ensemble) :
  1. Presidio (oracle : structuré + regex CNI + gazetteers) — toujours ;
  2. GLiNER zéro-shot — si types PERSON/LOC/ORG, mode recall_only ou
     demande explicite (policy.models) ;
  3. NER ouest-africain (SERENGETI / AfroXLMR / MasakhaNER 2.0) — si types
     PERSON/LOC/ORG ou demande explicite (policy.models) ; dégradation
     gracieuse si transformers ou le modèle est absent (F-43) ;
  4. Fusion : clustering IoS >= 0.5, vote pondéré, conflit de type
     (PERSON > LOC > ORG), span le plus long, filtrage seuils mode-aware,
     dédupe vs core_spans (le core fait foi) ;
  5. Expansion d'alias intra-session (si activée) ;
  6. Jauge quasi-id (si activée) — signal, jamais de résolution.

Le sidecar ne fait QUE DÉTECTER ; il ne lève jamais pour un détecteur
défaillant (dégradation gracieuse) et applique une deadline douce.
"""

from __future__ import annotations

import logging
import re
import time
from dataclasses import dataclass, field
from typing import Any, Sequence

from .african_models import AfricanModelDetector
from .alias import AliasExpander, AliasPatterns
from .config import Config
from .gliner_detect import GlinerDetector
from .presidio_oracle import PresidioOracle
from .quasi_id import QuasiIdGauge, QuasiIdReport
from .spans import Policy, SessionContext, Span, SpanType, iou

logger = logging.getLogger(__name__)

# Priorité de type en cas d'égalité de vote (PERSON est le plus sensible).
_TYPE_PRIORITY: tuple[SpanType, ...] = (SpanType.PERSON, SpanType.LOC, SpanType.ORG)

# Facteur appliqué aux seuils selon le mode de politique.
_MODE_THRESHOLD_FACTOR: dict[str, float] = {
    "balanced": 1.0,
    "recall_only": 0.85,
    "high_precision": 1.0,
}

_LOCALE_RE = re.compile(r"[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*")


@dataclass(frozen=True, slots=True)
class DetectRequest:
    """Requête normalisée (construite par les transports gRPC/REST)."""

    text: str
    locale: str = "fr"
    policy: Policy = field(default_factory=Policy)
    session: SessionContext = field(default_factory=SessionContext)
    core_spans: tuple[Span, ...] = ()


@dataclass(frozen=True, slots=True)
class DetectResponse:
    """Réponse normalisée (miroir wire : spans + quasi_id)."""

    spans: tuple[Span, ...]
    quasi_id: QuasiIdReport | None = None
    partial: bool = False  # deadline douce atteinte avant la fin des détecteurs (interne)


class DetectService:
    """Pipeline unique partagé par les transports gRPC et REST."""

    def __init__(self, config: Config) -> None:
        self.config = config
        self._presidio = PresidioOracle(config)
        self._gliner = GlinerDetector(config)
        self._african = AfricanModelDetector(config)
        self._expander = AliasExpander(AliasPatterns.from_config(config.alias))
        self._gauge = QuasiIdGauge(config)

    # -- état des modèles (healthz /models) -----------------------------------
    def model_status(self) -> dict[str, dict[str, Any]]:
        status = {
            "presidio": {
                "loaded": self._presidio.presidio_loaded(),
                "available": self._presidio.available(),
            },
            "gliner": {
                "loaded": self._gliner.loaded(),
                "available": self._gliner.available(),
            },
        }
        # modèles NER ouest-africains : état groupé + entrée par nom demandable
        # (policy.models={"serengeti"|"afroxlmr"|"masakha"} -> 503 si indisponible).
        african = self._african.status()
        status["african"] = dict(african)
        for name in self._african.supported_models:
            entry = dict(african)
            entry["model"] = name
            status[name] = entry
        return status

    def preload(self, level: str) -> None:
        """Précharge selon niveau : none | auto (presidio) | all (+ gliner, + africain)."""
        if level in ("auto", "all"):
            self._presidio.preload()
        if level == "all":
            self._gliner.preload()
            self._african.preload()

    # -- pipeline ---------------------------------------------------------------
    def detect(self, request: DetectRequest) -> DetectResponse:
        """Exécute le pipeline complet ; lève ValueError pour politique invalide."""
        self._validate(request)
        text, locale, policy = request.text, request.locale, request.policy

        budget = self.config.budget_seconds
        deadline = time.monotonic() + budget if budget > 0 else time.monotonic()
        skipped = False

        results: list[list[Span]] = []
        # 1) oracle Presidio (toujours — cascade de coût : léger)
        if self._budget_exhausted(deadline):
            skipped = True
        else:
            try:
                results.append(list(self._presidio.detect(text, locale, policy)))
            except Exception as exc:
                logger.warning("detect: presidio échoué (%s)", exc)
            skipped = skipped or self._budget_exhausted(deadline)

        # 2) GLiNER zéro-shot (lourd, lazy) si pertinent
        if self._wants_gliner(policy):
            if self._budget_exhausted(deadline):
                skipped = True
            else:
                try:
                    results.append(list(self._gliner.detect(text, locale, policy)))
                except Exception as exc:
                    logger.warning("detect: gliner échoué (%s)", exc)
                skipped = skipped or self._budget_exhausted(deadline)

        # 3) NER ouest-africain (SERENGETI / AfroXLMR / MasakhaNER 2.0, lazy)
        #    si pertinent ; dégradation gracieuse si transformers/modèle absent
        if self._wants_african(policy):
            if self._budget_exhausted(deadline):
                skipped = True
            else:
                try:
                    results.append(list(self._african.detect(text, locale, policy)))
                except Exception as exc:
                    logger.warning("detect: modèles africains échoués (%s)", exc)
                skipped = skipped or self._budget_exhausted(deadline)

        # 4) fusion + dédupe core_spans
        spans = self._fuse(text, results, policy, request.core_spans)
        # 5) alias intra-session
        if policy.enable_alias_expansion:
            spans = self._expander.expand(text, spans, request.session, policy, request.core_spans)
        # filtrage final (types demandés + seuils mode-aware, alias inclus)
        spans = self._apply_thresholds(spans, policy)
        # 6) jauge quasi-id
        quasi: QuasiIdReport | None = None
        if policy.enable_quasiid_gauge:
            quasi = self._gauge.evaluate(text, spans, request.core_spans, locale, policy)
        return DetectResponse(spans=tuple(spans), quasi_id=quasi, partial=skipped)

    # -- validation -------------------------------------------------------------
    @staticmethod
    def _validate(request: DetectRequest) -> None:
        if not isinstance(request.text, str):
            raise ValueError("text invalide")
        if not _LOCALE_RE.fullmatch(request.locale or ""):
            raise ValueError("locale mal formée")
        n = len(request.text)
        for s in request.core_spans:
            if not (0 <= s.start < s.end <= n):
                raise ValueError("offsets core_spans invalides")

    @staticmethod
    def _budget_exhausted(deadline: float) -> bool:
        return time.monotonic() >= deadline

    def _wants_gliner(self, policy: Policy) -> bool:
        if not self.config.gliner.enabled:
            return False
        if "gliner" in policy.models:
            return True
        if not policy.types:  # vide = tous les types
            return True
        return any(t in (SpanType.PERSON, SpanType.LOC, SpanType.ORG) for t in policy.types)

    def _wants_african(self, policy: Policy) -> bool:
        """NER ouest-africain : types PERSON/LOC/ORG OU demande explicite
        (policy.models contient serengeti/afroxlmr/masakha)."""
        if not self.config.african.enabled:
            return False
        if any(m in policy.models for m in ("serengeti", "afroxlmr", "masakha")):
            return True
        if not policy.types:  # vide = tous les types
            return True
        return any(t in (SpanType.PERSON, SpanType.LOC, SpanType.ORG) for t in policy.types)

    # -- fusion -----------------------------------------------------------------
    def _fuse(
        self,
        text: str,
        results: Sequence[Sequence[Span]],
        policy: Policy,
        core_spans: Sequence[Span],
    ) -> list[Span]:
        candidates = [
            c
            for group in results
            for s in group
            for c in [self._normalize(text, s)]
            if c is not None
        ]
        resolved: list[Span] = []
        for cluster in self._cluster(candidates):
            span = self._resolve_cluster(cluster, policy)
            if span is not None:
                resolved.append(span)
        # dédupe vs core_spans : le core fait foi (IoS >= 0.5)
        kept = [s for s in resolved if not any(iou(s, c) >= 0.5 for c in core_spans)]
        return sorted(self._non_overlap(kept), key=lambda s: (s.start, s.end))

    @staticmethod
    def _normalize(text: str, s: Span) -> Span | None:
        """Borne l'offset au texte et retire les espaces périphériques."""
        start = max(0, min(s.start, len(text)))
        end = max(start, min(s.end, len(text)))
        while start < end and text[start].isspace():
            start += 1
        while end > start and text[end - 1].isspace():
            end -= 1
        if start >= end:
            return None
        return Span(
            start=start, end=end, type=s.type, score=s.score,
            source=s.source, alias_of=s.alias_of,
        )

    @staticmethod
    def _cluster(spans: list[Span]) -> list[list[Span]]:
        """Union-find : deux spans dans le même cluster si IoS >= 0.5."""
        parent = list(range(len(spans)))

        def find(x: int) -> int:
            while parent[x] != x:
                parent[x] = parent[parent[x]]
                x = parent[x]
            return x

        def union(a: int, b: int) -> None:
            ra, rb = find(a), find(b)
            if ra != rb:
                parent[rb] = ra

        for i in range(len(spans)):
            for j in range(i + 1, len(spans)):
                if iou(spans[i], spans[j]) >= 0.5:
                    union(i, j)
        groups: dict[int, list[Span]] = {}
        for i, s in enumerate(spans):
            groups.setdefault(find(i), []).append(s)
        return list(groups.values())

    def _resolve_cluster(self, cluster: list[Span], policy: Policy) -> Span | None:
        # vote pondéré par type
        votes: dict[SpanType, float] = {}
        sources: set[str] = set()
        for s in cluster:
            sources.add(s.source.split(":")[0])
            votes[s.type] = votes.get(s.type, 0.0) + self._weight_of(s)
        if not votes:
            return None
        winner = max(votes, key=lambda t: (votes[t], -self._type_rank(t)))
        # score cluster = moyenne pondérée des scores présents
        numerator = sum(self._weight_of(s) * s.score for s in cluster)
        denominator = sum(self._weight_of(s) for s in cluster)
        score = (numerator / denominator) if denominator > 0 else 0.0
        score = max(0.0, min(1.0, score))
        # span représentatif : le plus long parmi les scores maximaux
        best = max(cluster, key=lambda s: (s.score, s.end - s.start, -s.start))
        # filtrage mode-aware
        if policy.mode == "high_precision" and "presidio" not in sources:
            return None
        threshold = policy.threshold_for(winner) * _MODE_THRESHOLD_FACTOR.get(policy.mode, 1.0)
        if score < threshold:
            return None
        return Span(
            start=best.start, end=best.end, type=winner,
            score=round(score, 4), source="ensemble",
        )

    def _weight_of(self, s: Span) -> float:
        key = s.source.split(":")[0]
        return float(getattr(self.config.weights, key, 1.0))

    @staticmethod
    def _type_rank(t: SpanType) -> int:
        return _TYPE_PRIORITY.index(t) if t in _TYPE_PRIORITY else len(_TYPE_PRIORITY)

    @staticmethod
    def _non_overlap(spans: list[Span]) -> list[Span]:
        """Résout les chevauchements résiduels entre clusters (IoS < 0.5) :
        garde le score le plus élevé, déterministe."""
        ordered = sorted(spans, key=lambda s: (s.start, -s.end, s.score))
        kept: list[Span] = []
        for s in ordered:
            while kept and kept[-1].start < s.end and s.start < kept[-1].end:
                if s.score > kept[-1].score:
                    kept.pop()
                else:
                    s = None
                    break
            if s is not None:
                kept.append(s)
        return kept

    @staticmethod
    def _apply_thresholds(spans: Sequence[Span], policy: Policy) -> list[Span]:
        """Filtre final : types demandés, pas d'UNKNOWN, seuils mode-aware."""
        factor = _MODE_THRESHOLD_FACTOR.get(policy.mode, 1.0)
        out: list[Span] = []
        for s in spans:
            if s.type is SpanType.UNKNOWN:
                continue
            if policy.types and s.type not in policy.types:
                continue
            if policy.mode == "high_precision" and s.source == "alias":
                continue  # l'alias n'a pas de consensus Presidio
            if s.score >= policy.threshold_for(s.type) * factor:
                out.append(s)
        return out
