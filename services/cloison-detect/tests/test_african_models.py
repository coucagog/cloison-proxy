"""Tests du détecteur NER ouest-africain (AfricanModelDetector, F-43) — STUBS.

Aucun réseau, aucun gros modèle : transformers est optionnel et le détecteur
se dégrade en stub (available() False, detect() -> []). Les tests
d'intégration remplacent l'instance interne par un fake, même pattern que
tests/test_detect_service.py (CLOISON_OFFLINE=1 via conftest).
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pytest

import src.african_models as african_mod
from src.config import AfricanConfig, Config
from src.detect_service import DetectRequest, DetectService
from src.spans import Policy, Span, SpanType


class StubPresidio:
    """Stub minimal de PresidioOracle : spans fixes, aucun chargement."""

    name = "presidio"

    def __init__(self, spans=None):
        self.spans = spans if spans is not None else []
        self.calls = 0

    def detect(self, text, locale="fr", policy=None):
        self.calls += 1
        if not text:
            return []
        return list(self.spans)

    def available(self):
        return True

    def presidio_loaded(self):
        return True


class StubGliner:
    """Fake minimal de GlinerDetector : jamais pertinent dans ces tests."""

    name = "gliner"

    def __init__(self, spans=None):
        self.spans = spans if spans is not None else []
        self.calls = 0

    def detect(self, text, locale="fr", policy=None):
        self.calls += 1
        return list(self.spans)

    def available(self):
        return False

    def loaded(self):
        return False


class StubAfrican:
    """Fake d'AfricanModelDetector : spans fixes ou indisponible."""

    name = "african"
    supported_models = ("serengeti", "afroxlmr", "masakha")

    def __init__(self, spans=None, available=True):
        self.spans = spans if spans is not None else []
        self.available_flag = available
        self.calls = 0

    def detect(self, text, locale="fr", policy=None):
        self.calls += 1
        return list(self.spans)

    def available(self):
        return self.available_flag

    def loaded(self):
        return self.available_flag

    def status(self):
        return {
            "loaded": self.loaded(),
            "available": self.available(),
            "model": "serengeti",
            "model_id": None,
        }


def make_service(presidio=None, gliner=None, african=None, **config_over) -> DetectService:
    cfg = Config(**config_over)
    svc = DetectService(cfg)
    svc._presidio = presidio if presidio is not None else StubPresidio()
    svc._gliner = gliner if gliner is not None else StubGliner()
    svc._african = african if african is not None else StubAfrican()
    return svc


# ---------------------------------------------------------------------------
# AfricanModelDetector — stub / dégradation gracieuse
# ---------------------------------------------------------------------------


def test_stub_transformers_absent_degrades(monkeypatch):
    """transformers absent -> stub : jamais disponible, detect() -> []."""
    monkeypatch.setattr(african_mod, "AutoModelForTokenClassification", None)
    monkeypatch.setattr(african_mod, "AutoTokenizer", None)
    det = african_mod.AfricanModelDetector(Config())
    assert det.available() is False
    assert det.loaded() is False
    assert det.detect("Marie habite à Ouagadougou.") == []


def test_stub_unknown_model_name_degrades():
    """modèle absent/inconnu -> available() False, detect() -> [] (aucun crash)."""
    cfg = Config(african=AfricanConfig(model_name="inconnu"))
    det = african_mod.AfricanModelDetector(cfg)
    assert det.available() is False
    assert det.detect("Marie habite à Ouagadougou.") == []


def test_label_mapping_bio_and_plain():
    """Labels NER (BIO et plain, fr/en) -> SpanType canonique."""
    to_type = african_mod.AfricanModelDetector._type_of_label
    assert to_type("B-PER") is SpanType.PERSON
    assert to_type("I-PERSON") is SpanType.PERSON
    assert to_type("B-LOCATION") is SpanType.LOC
    assert to_type("I-GPE") is SpanType.LOC
    assert to_type("B-ORG") is SpanType.ORG
    assert to_type("ORGANIZATION") is SpanType.ORG
    assert to_type("ORGANISATION") is SpanType.ORG
    assert to_type("O") is None
    assert to_type("") is None


def test_source_name_follows_model():
    """La source pour la fusion suit le modèle configuré (poids serengeti/afro)."""
    det = african_mod.AfricanModelDetector(Config(african=AfricanConfig(model_name="serengeti")))
    assert det.name == "serengeti"
    for name in ("afroxlmr", "masakha"):
        det = african_mod.AfricanModelDetector(Config(african=AfricanConfig(model_name=name)))
        assert det.name == "afro"


# ---------------------------------------------------------------------------
# Intégration DetectService — fake AfricanModelDetector
# ---------------------------------------------------------------------------


def test_african_span_included_in_fusion():
    """Un span PERSON du détecteur africain survit à la fusion."""
    african = StubAfrican([Span(0, 5, SpanType.PERSON, 0.9, source="serengeti")])
    svc = make_service(presidio=StubPresidio([]), african=african)
    req = DetectRequest(
        text="Marie habite à Ouagadougou.", locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON})),
    )
    res = svc.detect(req)
    assert african.calls == 1
    person = [s for s in res.spans if s.type is SpanType.PERSON]
    assert person and (person[0].start, person[0].end) == (0, 5)


def test_african_called_when_model_requested_explicitly():
    """policy.models=['serengeti'] déclenche le détecteur même sans types PERSON."""
    african = StubAfrican([Span(0, 5, SpanType.PERSON, 0.9, source="serengeti")])
    svc = make_service(presidio=StubPresidio([]), african=african)
    req = DetectRequest(text="Marie.", locale="fr", policy=Policy(models=("serengeti",)))
    res = svc.detect(req)
    assert african.calls == 1
    assert any(s.type is SpanType.PERSON for s in res.spans)


def test_african_not_called_for_unrelated_types():
    african = StubAfrican()
    svc = make_service(presidio=StubPresidio([]), african=african)
    req = DetectRequest(text="x", locale="fr", policy=Policy(types=frozenset({SpanType.DATE})))
    res = svc.detect(req)
    assert african.calls == 0
    assert res.spans == ()


# ---------------------------------------------------------------------------
# Dégradation : modèles africains demandés mais indisponibles
# ---------------------------------------------------------------------------


def test_degradation_policy_serengeti_no_model():
    """models=['serengeti'] sans modèle dispo : pas de crash, spans vides."""
    svc = make_service(presidio=StubPresidio([]), african=StubAfrican(available=False))
    req = DetectRequest(text="Marie", locale="fr", policy=Policy(models=("serengeti",)))
    res = svc.detect(req)  # ne doit pas lever
    assert res.spans == ()


def test_rest_requested_serengeti_unavailable_503():
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from src.api import create_app

    svc = make_service(presidio=StubPresidio([]), african=StubAfrican(available=False))
    client = TestClient(create_app(svc))
    payload = {"text": "abc", "locale": "fr", "policy": {"models": ["serengeti"]}}
    r = client.post("/detect", json=payload)
    assert r.status_code == 503
    assert r.json()["error"]["code"] == "FAILED_PRECONDITION"


def test_model_status_reports_african_models():
    """model_status() expose l'état des modèles africains (+ aliases demandables)."""
    svc = make_service(presidio=StubPresidio([]), african=StubAfrican(available=False))
    status = svc.model_status()
    assert status["african"]["available"] is False
    assert status["african"]["loaded"] is False
    for name in ("serengeti", "afroxlmr", "masakha"):
        assert name in status
        assert status[name]["available"] is False


# ---------------------------------------------------------------------------
# Régression DEPLOY-6 : `offset_mapping` ne doit jamais atteindre forward()
# ---------------------------------------------------------------------------


def test_african_detect_never_passes_offset_mapping_to_model():
    """Le tokenizer renvoie `offset_mapping` (return_offsets_mapping=True),
    mais `XLMRobertaForTokenClassification.forward()` n'accepte PAS ce kwarg
    (signature sans **kwargs) : le passer levait une TypeError silencieuse et
    le détecteur africain renvoyait [] (bug DEPLOY-6 — venv non-pinné STACK-8
    tolérait le kwarg, transformers 4.46.3 épinglé non)."""
    import torch

    class FakeTokenizer:
        def __call__(self, text, **kwargs):  # noqa: ARG002
            # Même shape que le vrai tokenizer avec return_offsets_mapping=True.
            return {
                "input_ids": torch.tensor([[0, 5, 9, 2]]),
                "attention_mask": torch.tensor([[1, 1, 1, 1]]),
                "offset_mapping": torch.tensor([[(0, 0), (0, 5), (5, 9), (0, 0)]]),
            }

    class FakeLogits:
        # [1, 4, 3] : tokens 1-2 → label 2 (LOC), tokens 0/3 spéciaux.
        logits = torch.tensor([
            [[0.1, 0.1, 0.8], [0.1, 0.1, 0.8], [0.1, 0.1, 0.8], [0.9, 0.05, 0.05]]
        ])

    class FakeModel:
        class Config:
            id2label = {0: "O", 1: "PER", 2: "LOC"}

        config = Config()

        def __call__(self, **kwargs):
            assert "offset_mapping" not in kwargs, (
                "offset_mapping ne doit jamais atteindre forward()"
            )
            return FakeLogits()

    det = african_mod.AfricanModelDetector(
        Config(african=AfricanConfig(model_name="afroxlmr"))
    )
    det._tokenizer = FakeTokenizer()
    det._model = FakeModel()
    det._load_attempted = True  # contourne le chargement réel

    spans = det.detect("Bonjour Aminata Diop à Dakar", locale="fr", policy=None)
    assert spans, "le détecteur africain doit produire des spans (plus de [] silencieux)"
    assert spans[0].type is SpanType.LOC
    assert spans[0].score >= 0.50, "score au-dessus du seuil interne african"
