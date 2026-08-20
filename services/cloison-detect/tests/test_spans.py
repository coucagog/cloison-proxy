"""Tests du module types (src/spans.py) : Span, SpanType, Policy, IoS, JSON.

Aucun réseau, aucun modèle : uniquement du code pur.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pytest

from src.spans import (
    CanonicalMention,
    Policy,
    SessionContext,
    Span,
    SpanType,
    iou,
    normalize_text,
)


def test_span_valid():
    s = Span(start=0, end=4, type=SpanType.PERSON, score=0.9)
    assert s.start == 0 and s.end == 4 and s.type is SpanType.PERSON and s.score == 0.9


def test_span_invalid_bounds():
    with pytest.raises(ValueError):
        Span(start=4, end=4, type=SpanType.PERSON, score=0.9)   # start == end
    with pytest.raises(ValueError):
        Span(start=-1, end=4, type=SpanType.PERSON, score=0.9)  # négatif
    with pytest.raises(ValueError):
        Span(start=5, end=4, type=SpanType.PERSON, score=0.9)   # inversé


def test_span_invalid_score():
    with pytest.raises(ValueError):
        Span(start=0, end=4, type=SpanType.PERSON, score=1.5)
    with pytest.raises(ValueError):
        Span(start=0, end=4, type=SpanType.PERSON, score=-0.1)


def test_span_type_parse():
    assert SpanType.parse("PERSON") is SpanType.PERSON
    assert SpanType.parse("person") is SpanType.PERSON
    assert SpanType.parse("Loc") is SpanType.LOC
    assert SpanType.parse("DATE") is SpanType.DATE
    assert SpanType.parse("XYZ_UNKNOWN") is SpanType.UNKNOWN
    assert SpanType.parse("") is SpanType.UNKNOWN


def test_span_json_roundtrip():
    s = Span(start=0, end=12, type=SpanType.PERSON, score=0.93, source="presidio")
    d = s.to_dict()
    assert d["start"] == 0 and d["end"] == 12 and d["type"] == "PERSON" and d["score"] == 0.93
    assert Span.from_dict(d) == s
    assert Span.from_json(s.to_json()) == s


def test_iou():
    a = Span(0, 12, SpanType.PERSON, 1.0)
    assert iou(a, Span(0, 12, SpanType.PERSON, 1.0)) == pytest.approx(1.0)   # identique
    assert iou(a, Span(6, 12, SpanType.PERSON, 1.0)) == pytest.approx(1.0)   # sous-span
    assert iou(a, Span(12, 20, SpanType.PERSON, 1.0)) == pytest.approx(0.0)  # disjoint
    assert iou(a, Span(10, 15, SpanType.PERSON, 1.0)) == pytest.approx(2 / 5)


def test_utf8_safe_offsets():
    # les offsets Python sont des points de code : jamais de split d'octet UTF-8
    text = "Awa ☕ Diallo"
    assert len(text) == 12
    s = Span(start=0, end=len("Awa"), type=SpanType.PERSON, score=0.8)
    assert text[s.start:s.end] == "Awa"


def test_policy_defaults():
    p = Policy()
    assert p.min_score == 0.40
    assert p.mode == "balanced"
    assert p.enable_alias_expansion is True
    assert p.enable_quasiid_gauge is False
    assert p.quasiid_threshold == 0.50
    assert p.threshold_for(SpanType.PERSON) == 0.40
    assert p.wants_type(SpanType.PERSON) is True  # types vides = tous


def test_policy_thresholds_and_wants():
    p = Policy(types=frozenset({SpanType.PERSON, SpanType.LOC}),
               thresholds={SpanType.LOC: 0.35})
    assert p.threshold_for(SpanType.LOC) == 0.35
    assert p.threshold_for(SpanType.PERSON) == 0.40
    assert p.wants_type(SpanType.PERSON) is True
    assert p.wants_type(SpanType.ORG) is False


def test_policy_from_dict():
    p = Policy.from_dict({
        "types": ["PERSON", "LOC"],
        "min_score": 0.5,
        "thresholds": {"LOC": 0.3},
        "mode": "recall_only",
        "enable_alias_expansion": False,
        "enable_quasiid_gauge": True,
        "quasiid_threshold": 0.6,
    })
    assert p.types == frozenset({SpanType.PERSON, SpanType.LOC})
    assert p.min_score == 0.5
    assert p.mode == "recall_only"
    assert p.enable_alias_expansion is False
    assert p.enable_quasiid_gauge is True
    assert p.quasiid_threshold == 0.6
    assert Policy.from_dict(None) == Policy()


def test_policy_invalid():
    with pytest.raises(ValueError):
        Policy(mode="bogus")
    with pytest.raises(ValueError):
        Policy(min_score=2.0)
    with pytest.raises(ValueError):
        Policy(quasiid_threshold=1.5)


def test_normalize_text():
    assert normalize_text("OUAGADOUGOU") == "ouagadougou"
    assert normalize_text("Ouagadougou") == "ouagadougou"
    assert normalize_text("Aïcha") == "aicha"
    assert normalize_text("N'Djamena") == "n'djamena"


def test_mention_and_session_roundtrip():
    m = CanonicalMention(key="Marie Dupont", type=SpanType.PERSON, locale="fr-BF", seen_count=3)
    s = SessionContext(mentions=(m,))
    assert s.to_dict()["mentions"][0]["key"] == "Marie Dupont"
    assert SessionContext.from_dict(s.to_dict()) == s
    with pytest.raises(ValueError):
        CanonicalMention(key="  ", type=SpanType.PERSON)
    with pytest.raises(ValueError):
        CanonicalMention(key="Awa", type=SpanType.PERSON, seen_count=0)
