"""Tests de la jauge de quasi-identifiants : densité, fenêtrage, seuil,
zéro résolution. Aucun réseau, aucun modèle : code pur.

Invariant clé : la sortie ne contient jamais de valeurs ni d'identité.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from src.config import Config
from src.quasi_id import QuasiIdGauge
from src.spans import Policy, Span, SpanType


def make_gauge(window: int = 160, step: int = 40) -> QuasiIdGauge:
    cfg = Config()
    cfg.gauge.window = window
    cfg.gauge.step = step
    return QuasiIdGauge(cfg)


def loc_span(text: str, name: str) -> Span:
    idx = text.find(name)
    assert idx >= 0, f"{name!r} absent du texte"
    return Span(idx, idx + len(name), SpanType.LOC, 1.0, source="core")


def test_disabled_policy_returns_none():
    g = make_gauge()
    assert g.evaluate("42 ans", [], [], "fr", Policy(enable_quasiid_gauge=False)) is None


def test_dense_text_flagged():
    g = make_gauge()
    text = "Mamadou, 42 ans, acte n° 1847, enregistré le 12/03/2021 à Ouagadougou."
    loc = loc_span(text, "Ouagadougou")
    core = [Span(9, 15, SpanType.AGE, 1.0, source="core"), loc]
    report = g.evaluate(text, [], core, "fr",
                        Policy(enable_quasiid_gauge=True, quasiid_threshold=0.5))
    assert report is not None
    assert report.score >= 0.5
    assert report.flagged is True
    assert set(report.signals) == {"age", "act", "date", "loc"}


def test_sparse_text_not_flagged():
    g = make_gauge()
    report = g.evaluate("Bonjour, comment allez-vous ?", [], [], "fr",
                        Policy(enable_quasiid_gauge=True, quasiid_threshold=0.5))
    assert report is not None
    assert report.score == 0.0
    assert report.flagged is False
    assert report.signals == ()


def test_threshold_1_disable_flag():
    g = make_gauge()
    text = "42 ans, acte n° 5, le 12/03/2021 à Ouagadougou."
    core = [loc_span(text, "Ouagadougou")]
    report = g.evaluate(text, [], core, "fr",
                        Policy(enable_quasiid_gauge=True, quasiid_threshold=1.0))
    assert report is not None
    assert report.score >= 0.5
    assert report.flagged is False   # seuil 1.0 = jauge désactivée de fait


def test_windowing_max_over_windows():
    g = make_gauge(window=40, step=10)
    text = "Bonjour. " * 20 + "Il a 42 ans, acte n° 7, le 12/03/2021 à Ouagadougou."
    core = [loc_span(text, "Ouagadougou")]
    report = g.evaluate(text, [], core, "fr",
                        Policy(enable_quasiid_gauge=True, quasiid_threshold=0.5))
    assert report is not None
    assert report.flagged is True
    assert report.score >= 0.5


def test_no_resolution_no_values():
    g = make_gauge()
    text = "Awa, 42 ans, acte n° 1847, le 12/03/2021, Ouagadougou."
    core = [Span(0, 3, SpanType.PERSON, 1.0, source="core"), loc_span(text, "Ouagadougou")]
    report = g.evaluate(text, [], core, "fr", Policy(enable_quasiid_gauge=True))
    assert report is not None
    # la jauge ne renvoie que des catégories : jamais de valeur, jamais d'identité
    assert isinstance(report.score, float) and 0.0 <= report.score <= 1.0
    assert isinstance(report.flagged, bool)
    assert set(report.signals) <= set(g.CATEGORIES)
    assert report.signals == tuple(c for c in g.CATEGORIES if c in report.signals)  # ordre stable


def test_empty_text():
    g = make_gauge()
    report = g.evaluate("", [], [], "fr", Policy(enable_quasiid_gauge=True))
    assert report is not None
    assert report.score == 0.0
    assert report.flagged is False
    assert report.signals == ()


def test_signals_from_sidecar_spans():
    g = make_gauge()
    text = "Rendez-vous le 3 mars 2021 dans la ville."
    date_start = text.find("3 mars 2021")
    loc_start = text.find("ville")
    sidecar = [Span(date_start, date_start + len("3 mars 2021"), SpanType.DATE, 0.9, source="presidio"),
               Span(loc_start, loc_start + len("ville"), SpanType.LOC, 0.8, source="gliner")]
    report = g.evaluate(text, sidecar, [], "fr",
                        Policy(enable_quasiid_gauge=True, quasiid_threshold=0.5))
    assert report is not None
    assert {"date", "loc"} <= set(report.signals)
