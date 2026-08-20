"""Tests de l'expansion d'alias intra-session (R1–R7, sécurité pronoms).

Aucun réseau, aucun modèle : AliasExpander est du code pur.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pytest

from src.alias import AliasExpander, AliasPatterns
from src.config import AliasConfig
from src.spans import CanonicalMention, Policy, SessionContext, Span, SpanType


def make_expander(**over) -> AliasExpander:
    return AliasExpander(AliasPatterns.from_config(AliasConfig(**over)))


def person(key: str, seen: int = 1) -> CanonicalMention:
    return CanonicalMention(key=key, type=SpanType.PERSON, locale="fr", seen_count=seen)


# ---------------------------------------------------------------------------
# Dérivation (R1–R7)
# ---------------------------------------------------------------------------


def test_derive_r1_r2_r3():
    exp = make_expander()
    forms = exp.derive(person("Marie Dupont", seen=3))
    assert "Marie" in forms          # R1 prénom seul
    assert "Dupont" in forms         # R3 nom seul
    assert "Mme Dupont" in forms     # R2 titre + nom
    assert "M. Dupont" in forms      # R2
    assert "Madame Dupont" in forms  # R2


def test_derive_excludes_common_names():
    exp = make_expander()
    forms = exp.derive(person("Marie Les"))
    assert "Les" not in forms        # R3 : « Les » est un nom trop commun
    assert "Marie" in forms          # R1 reste


def test_derive_pronoun_never_leaks():
    exp = make_expander()
    assert exp.derive(person("il")) == frozenset()
    assert exp.derive(person("Elle")) == frozenset()
    assert exp.derive(person("on")) == frozenset()


def test_derive_title_first_token_not_alias():
    exp = make_expander()
    forms = exp.derive(person("M. Dupont"))
    assert "M." not in forms         # R1 : un titre n'est pas un prénom
    assert "Dupont" in forms


def test_derive_diminutives_r5():
    exp = make_expander(diminutives={"Momo": "Mamadou"})
    forms = exp.derive(person("Mamadou"))
    assert "Momo" in forms


def test_derive_place_shortcuts_r6():
    exp = make_expander(place_shortcuts={"Ouaga": "Ouagadougou"})
    m = CanonicalMention(key="Ouagadougou", type=SpanType.LOC, locale="fr")
    assert "Ouaga" in exp.derive(m)


def test_derive_initial_forms_off_by_default():
    exp = make_expander()
    assert "Marie D." not in exp.derive(person("Marie Dupont"))
    exp2 = make_expander(enable_initial_forms=True)
    assert "Marie D." in exp2.derive(person("Marie Dupont"))


def test_derive_max_forms_guard():
    exp = make_expander(
        titles=("M.", "Mme", "Mlle", "Dr", "Pr", "Prof", "Col", "Cdt",
                "Sergent", "Major", "Général", "Générale"),
    )
    forms = exp.derive(person("Marie Dupont"))
    assert 0 < len(forms) <= 8


# ---------------------------------------------------------------------------
# Expansion
# ---------------------------------------------------------------------------


def test_expand_empty_session_noop():
    exp = make_expander()
    spans = [Span(0, 12, SpanType.PERSON, 0.9, source="presidio")]
    out = exp.expand("Marie est partie", spans, SessionContext(), Policy())
    assert out == tuple(spans)


def test_expand_disabled_policy_noop():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont"),))
    out = exp.expand("Marie est partie", [], session,
                     Policy(enable_alias_expansion=False))
    assert out == ()


def test_expand_alias_basic():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont", seen=1),))
    text = "Marie Dupont est partie. Marie reviendra. Mme Dupont aussi."
    spans = [Span(0, 12, SpanType.PERSON, 0.9, source="presidio")]
    out = exp.expand(text, spans, session, Policy())
    alias = [(text[s.start:s.end], s.alias_of) for s in out if s.source == "alias"]
    assert ("Marie", "Marie Dupont") in alias      # R1
    assert ("Mme Dupont", "Marie Dupont") in alias  # R2
    for s in out:
        if s.source == "alias":
            assert s.start >= 12                          # pas de chevauchement canonique
            assert s.score <= 0.9 * 0.85 + 1e-9           # score plafonné ×0.85
            assert s.alias_of == "Marie Dupont"


def test_expand_score_cap_and_seen_boost():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont", seen=10),))
    text = "Marie Dupont est là. Dupont aussi."
    spans = [Span(0, 12, SpanType.PERSON, 1.0, source="presidio")]
    out = exp.expand(text, spans, session, Policy())
    alias = [s for s in out if s.source == "alias"]
    assert len(alias) == 1
    assert text[alias[0].start:alias[0].end] == "Dupont"
    assert alias[0].score <= 0.95 + 1e-9                 # plafond absolu
    assert alias[0].score <= 1.0 * 0.85 * 1.10 + 1e-9    # ×0.85 × boost borné
    assert alias[0].score > 0.85                          # le seen_count booste (> ×0.85)


def test_expand_accent_case_insensitive_r7():
    exp = make_expander()
    session = SessionContext(mentions=(person("Awa Diallo"),))
    text = "AWA DIALLO est là. awa est partie."
    spans = [Span(0, 11, SpanType.PERSON, 0.95, source="presidio")]
    out = exp.expand(text, spans, session, Policy())
    alias_texts = [text[s.start:s.end] for s in out if s.source == "alias"]
    assert "awa" in alias_texts


def test_expand_pronoun_never_matched():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont"),))
    out = exp.expand("il est parti. elle aussi.", [], session, Policy())
    assert all(s.source != "alias" for s in out)


def test_expand_alias_without_canonical_span():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont"),))
    text = "Marie est partie."          # la mention canonique n'apparaît pas ici
    out = exp.expand(text, [], session, Policy())
    alias = [s for s in out if s.source == "alias"]
    assert len(alias) == 1
    assert text[alias[0].start:alias[0].end] == "Marie"
    assert alias[0].score == pytest.approx(0.80 * 0.85, abs=1e-6)  # score canonique par défaut


def test_expand_dedupe_against_core_spans():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont"),))
    text = "Marie Dupont est là. Marie."
    start = text.find("Marie", 12)
    core = (Span(start, start + 5, SpanType.PERSON, 1.0, source="core"),)
    out = exp.expand(text, [Span(0, 12, SpanType.PERSON, 0.9)], session, Policy(), core_spans=core)
    assert all(s.source != "alias" for s in out)   # le « Marie » final est couvert par le core


def test_expand_deterministic():
    exp = make_expander()
    session = SessionContext(mentions=(person("Marie Dupont", seen=2), person("Mamadou Diallo")))
    text = "Marie et Mamadou sont là. Mme Dupont suit. Momo aussi."
    spans = [Span(0, 12, SpanType.PERSON, 0.9, source="presidio")]
    first = exp.expand(text, spans, session, Policy())
    second = exp.expand(text, spans, session, Policy())
    assert first == second                            # mêmes entrées -> mêmes sorties
