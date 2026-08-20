"""Jauge de quasi-identifiants (API_DESIGN §6) : SIGNAL, jamais de résolution.

Densité de catégories (age + act + date + lieu) dans une fenêtre glissante.
La sortie ne contient ni valeurs, ni identité reconstituée, ni chaînage :
elle signale une densité que le core seul peut interpréter (avertissement,
refus, k-anonymat renforcé).
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Sequence

from .config import Config
from .spans import Policy, Span, SpanType

# Type de span -> catégorie de signal. PERSON/ORG/ID ne sont PAS des
# quasi-identifiants pris en compte ici (liste fermée de 4 catégories).
_SIGNAL_BY_TYPE: dict[SpanType, str] = {
    SpanType.AGE: "age",
    SpanType.ACT: "act",
    SpanType.DATE: "date",
    SpanType.LOC: "loc",
}

# Regex internes simples (le core fournit déjà les spans structurés ; ces
# regex couvrent les cas où le sidecar voit les mentions sans spans core).
_RE_AGE = re.compile(
    r"\b\d{1,3}\s*(?:ans?|an)\b|\b(?:née?|né)\s+en\s+\d{4}\b", re.IGNORECASE
)
_RE_ACT = re.compile(
    r"\bacte\s*(?:n[°o]?|n\.)?\s*\d+(?:\s*/\s*\d{2,4})?\b", re.IGNORECASE
)
_RE_DATE = re.compile(
    r"\b\d{1,2}[/-]\d{1,2}[/-]\d{2,4}\b"
    r"|\b\d{4}[/-]\d{1,2}[/-]\d{1,2}\b"
    r"|\b\d{1,2}\s+(?:janvier|février|mars|avril|mai|juin|juillet|août"
    r"|septembre|octobre|novembre|décembre)\s+\d{4}\b",
    re.IGNORECASE,
)


@dataclass(frozen=True, slots=True)
class QuasiIdReport:
    """Rapport de la jauge : densité normalisée + flag + catégories.

    Jamais de valeurs, jamais d'identité reconstituée.
    """

    score: float
    flagged: bool
    signals: tuple[str, ...] = ()

    def to_dict(self) -> dict[str, Any]:
        return {"score": self.score, "flagged": self.flagged, "signals": list(self.signals)}


class QuasiIdGauge:
    """Jauge de densité de quasi-identifiants (fenêtre glissante)."""

    CATEGORIES: tuple[str, ...] = ("age", "act", "date", "loc")

    def __init__(self, config: Config) -> None:
        self._window = config.gauge.window
        self._step = config.gauge.step
        self._max_bonus = config.gauge.max_bonus

    def evaluate(
        self,
        text: str,
        spans: Sequence[Span],
        core_spans: Sequence[Span],
        locale: str,
        policy: Policy,
    ) -> QuasiIdReport | None:
        """Calcule le rapport ; None si policy.enable_quasiid_gauge est faux.

        `locale` est réservé pour des regex de dates/langues spécifiques ;
        la version actuelle est agnostique à la locale.
        """
        if not policy.enable_quasiid_gauge:
            return None

        intervals: dict[str, list[tuple[int, int]]] = {c: [] for c in self.CATEGORIES}
        for s in list(spans) + list(core_spans):
            cat = _SIGNAL_BY_TYPE.get(s.type)
            if cat is not None:
                intervals[cat].append((s.start, s.end))
        for cat, pattern in (("age", _RE_AGE), ("act", _RE_ACT), ("date", _RE_DATE)):
            for m in pattern.finditer(text):
                intervals[cat].append((m.start(), m.end()))

        windows = self._windows(len(text))
        best_score = 0.0
        best_signals: set[str] = set()
        for w_start, w_end in windows:
            present = {
                cat
                for cat, ivs in intervals.items()
                if any(s < w_end and e > w_start for s, e in ivs)
            }
            count = sum(
                1
                for ivs in intervals.values()
                for s, e in ivs
                if s < w_end and e > w_start
            )
            density = len(present) / len(self.CATEGORIES)
            bonus = min(self._max_bonus, 0.1 * (count - 4)) if count > 4 else 0.0
            score = min(1.0, max(0.0, density + bonus))
            if score > best_score:
                best_score = score
                best_signals = set(present)

        order = tuple(c for c in self.CATEGORIES if c in best_signals)
        return QuasiIdReport(
            score=round(best_score, 4),
            flagged=best_score > policy.quasiid_threshold,
            signals=order,
        )

    def _windows(self, length: int) -> list[tuple[int, int]]:
        """Fenêtres glissantes [i, i+window), la dernière couvre la fin."""
        if length <= 0:
            return [(0, 0)]
        windows = [(i, min(i + self._window, length)) for i in range(0, length, self._step)]
        if windows[-1][1] < length:
            windows.append((max(0, length - self._window), length))
        return windows
