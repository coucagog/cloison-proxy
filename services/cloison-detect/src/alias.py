"""Expansion d'alias intra-session (API_DESIGN §5).

Règles R1–R7 : prénom seul, titre + nom, nom seul (hors noms communs),
prénom + initiale (off par défaut), diminutifs, raccourcis de lieux,
normalisation casse/diacritiques.

Invariants de sécurité :
- Aucune inférence hors contexte : session vide => no-op.
- Les pronoms et mots-outils ne sont JAMAIS dérivés ni matchés (pas de fuite).
- Score plafonné : alias <= score_cap * score canonique ; seen_count ne
  dépasse jamais max_score. Déterministe : mêmes entrées -> mêmes sorties.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass, field
from typing import Mapping, Sequence

from .config import AliasConfig
from .spans import (
    CanonicalMention,
    Policy,
    SessionContext,
    Span,
    SpanType,
    insensitive_pattern,
    normalize_text,
)

logger = logging.getLogger(__name__)


@dataclass(frozen=True, slots=True)
class AliasPatterns:
    """Règles d'alias (miroir de AliasConfig)."""

    titles: tuple[str, ...] = ("M.", "Mme", "Mlle", "Dr", "Pr", "Madame", "Monsieur")
    diminutives: Mapping[str, str] = field(default_factory=dict)      # "Momo" -> "Mamadou"
    place_shortcuts: Mapping[str, str] = field(default_factory=dict)  # "Ouaga" -> "Ouagadougou"
    common_names: frozenset[str] = frozenset()                        # noms trop communs (R3)
    pronouns: frozenset[str] = frozenset()                            # jamais de fuite
    max_derived_forms: int = 8                                        # garde-fou anti-explosion
    enable_initial_forms: bool = False                                # R4, off par défaut
    score_cap: float = 0.85
    max_score: float = 0.95
    default_canonical_score: float = 0.80

    @classmethod
    def from_config(cls, cfg: AliasConfig) -> "AliasPatterns":
        return cls(
            titles=cfg.titles,
            diminutives=cfg.diminutives,
            place_shortcuts=cfg.place_shortcuts,
            common_names=frozenset(cfg.common_names) | cfg.pronoun_forms,
            pronouns=cfg.pronoun_forms,
            max_derived_forms=cfg.max_derived_forms,
            enable_initial_forms=cfg.enable_initial_forms,
            score_cap=cfg.score_cap,
            max_score=cfg.max_score,
            default_canonical_score=cfg.default_canonical_score,
        )


class AliasExpander:
    """Stateless par requête : l'index d'alias est reconstruit à chaque expand().

    Le core reste propriétaire du store de mentions ; ce côté ne garde aucun
    état entre deux requêtes.
    """

    def __init__(self, patterns: AliasPatterns) -> None:
        self._patterns = patterns
        self._blocked_norm = frozenset(
            normalize_text(x) for x in (patterns.common_names | patterns.pronouns)
        )
        self._title_norms = frozenset(normalize_text(t) for t in patterns.titles)

    # ------------------------------------------------------------------ R1–R7
    def derive(self, m: CanonicalMention) -> frozenset[str]:
        """Formes dérivées (R1–R7) pour une mention canonique.

        R1 prénom seul · R2 titre + nom · R3 nom seul (hors noms communs) ·
        R4 prénom + initiale (off) · R5 diminutifs · R6 raccourcis de lieux ·
        R7 (casse/diacritiques) est appliquée au moment du matching.
        """
        key = m.key.strip()
        if not key:
            return frozenset()
        tokens = key.split()
        if not tokens:
            return frozenset()
        norm_key = normalize_text(key)
        if not norm_key or norm_key in self._blocked_norm:
            # un pronom / mot banal n'est jamais une mention exploitable
            return frozenset()

        forms: set[str] = set()
        first, last = tokens[0], tokens[-1]
        if len(tokens) >= 2:
            # R1 — prénom seul (jamais un titre, jamais un mot banal)
            if (
                len(first) >= 2
                and normalize_text(first) not in self._blocked_norm
                and normalize_text(first) not in self._title_norms
            ):
                forms.add(first)
            # R2 — titre + nom
            if len(last) >= 2:
                for title in self._patterns.titles:
                    forms.add(f"{title} {last}")
            # R3 — nom seul (hors noms communs / pronoms)
            if len(last) >= 2 and normalize_text(last) not in self._blocked_norm:
                forms.add(last)
            # R4 — prénom + initiale (optionnel, off par défaut)
            if self._patterns.enable_initial_forms:
                forms.add(f"{first} {last[0]}.")
                forms.add(f"{first[0]}. {last[0]}.")
        # R5 — diminutifs
        for dim, canon in self._patterns.diminutives.items():
            if normalize_text(canon) == norm_key:
                forms.add(dim)
        # R6 — raccourcis de lieux
        for short, long in self._patterns.place_shortcuts.items():
            if normalize_text(long) == norm_key:
                forms.add(short)

        clean = frozenset(
            f
            for f in forms
            if f.strip()
            and len(normalize_text(f)) >= 2
            and normalize_text(f) not in self._blocked_norm
            and normalize_text(f) != norm_key
        )
        if len(clean) > self._patterns.max_derived_forms:
            return frozenset(sorted(clean)[: self._patterns.max_derived_forms])
        return clean

    # ------------------------------------------------------------------ expand
    def expand(
        self,
        text: str,
        spans: Sequence[Span],
        session: SessionContext,
        policy: Policy,
        core_spans: Sequence[Span] = (),
    ) -> tuple[Span, ...]:
        """Enrichit `spans` des alias (source="alias", alias_of=key).

        Session vide ou politique désactivée -> spans inchangés. Chaque alias
        est dédupliqué contre les spans existants (sidecar + core). Un alias
        qui englobe strictement un span court l'étend ; un chevauchement
        partiel est ignoré (l'existant fait foi).
        """
        if not session.mentions or not policy.enable_alias_expansion or not text:
            return tuple(spans)

        index = self._build_index(session)       # forme normalisée -> (key, mention, forme)
        canonical_scores = self._canonical_scores(text, spans, session)
        existing = list(spans) + list(core_spans)
        working: list[Span] = list(spans)

        for norm_form in sorted(index, key=lambda f: (-len(f), f)):
            entries = index[norm_form]
            # une seule regex par forme (la première forme originale suffit)
            pattern = re.compile(
                r"(?<![\w-])" + insensitive_pattern(entries[0][2]) + r"(?![\w-])",
                re.IGNORECASE | re.UNICODE,
            )
            for match in pattern.finditer(text):
                for key, mention, _form in entries:
                    alias = self._build_alias_span(match.start(), match.end(), mention, key, canonical_scores)
                    if alias is None:
                        continue
                    working = self._merge_alias(working, existing, alias)
        return tuple(sorted(working, key=lambda s: (s.start, s.end)))

    # ------------------------------------------------------------------ helpers
    def _build_index(
        self, session: SessionContext
    ) -> dict[str, list[tuple[str, CanonicalMention, str]]]:
        """forme normalisée -> [(key canonique, mention, forme originale)]."""
        index: dict[str, list[tuple[str, CanonicalMention, str]]] = {}
        for m in session.mentions:
            for form in sorted(self.derive(m)):  # tri : déterministe inter-processus
                index.setdefault(normalize_text(form), []).append((m.key, m, form))
        return index

    def _canonical_scores(
        self, text: str, spans: Sequence[Span], session: SessionContext
    ) -> dict[str, float]:
        """Score de la mention canonique quand elle est spanée dans CE texte."""
        scores: dict[str, float] = {}
        for s in spans:
            if s.type not in (SpanType.PERSON, SpanType.LOC):
                continue
            fragment = normalize_text(text[s.start:s.end])
            for m in session.mentions:
                if fragment == normalize_text(m.key):
                    scores[m.key] = max(scores.get(m.key, 0.0), s.score)
        return scores

    def _build_alias_span(
        self,
        start: int,
        end: int,
        mention: CanonicalMention,
        key: str,
        canonical_scores: Mapping[str, float],
    ) -> Span | None:
        base = canonical_scores.get(key, self._patterns.default_canonical_score)
        # boost borné du seen_count : jamais au-delà de max_score ni du canonique
        boost = min(1.0 + 0.02 * max(0, mention.seen_count - 1), 1.10)
        score = min(base * self._patterns.score_cap * boost, self._patterns.max_score, base)
        return Span(
            start=start,
            end=end,
            type=mention.type,
            score=round(score, 4),
            source="alias",
            alias_of=key,
        )

    def _merge_alias(
        self, working: list[Span], existing: Sequence[Span], alias: Span
    ) -> list[Span]:
        """Dédoupe : couvert -> ignoré ; englobe strictement -> extension ;
        chevauchement partiel -> l'existant fait foi."""
        if any(s.start <= alias.start and alias.end <= s.end for s in existing):
            return working
        overlapping = [i for i, s in enumerate(working) if alias.start < s.end and s.start < alias.end]
        if not overlapping:
            working.append(alias)
            return working
        if all(
            alias.start <= working[i].start and working[i].end <= alias.end
            for i in overlapping
        ):
            kept = [s for i, s in enumerate(working) if i not in overlapping]
            kept.append(alias)
            return kept
        return working
