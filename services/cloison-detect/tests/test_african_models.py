"""Tests du détecteur NER ouest-africain (AfricanModelDetector, F-43) — STUBS.

Aucun réseau, aucun gros modèle : transformers est optionnel et le détecteur
se dégrade en stub (available() False, detect() -> []). Les tests
d'intégration remplacent l'instance interne par un fake, même pattern que
tests/test_detect_service.py (CLOISON_OFFLINE=1 via conftest).
"""

from __future__ import annotations

import sys
import types
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


# ---------------------------------------------------------------------------
# Voie ONNX (dette ③, journal DEPLOY-8) — backend ONNX Runtime, fallback torch
# ---------------------------------------------------------------------------


class FakeTokenizerNp:
    """Fake tokenizer numpy (return_tensors="np") — mêmes shapes que le torch."""

    def __call__(self, text, **kwargs):  # noqa: ARG002
        import numpy as np

        return {
            "input_ids": np.array([[0, 5, 9, 2]]),
            "attention_mask": np.array([[1, 1, 1, 1]]),
            "offset_mapping": np.array([[(0, 0), (0, 5), (5, 9), (0, 0)]]),
        }


class FakeOrtSession:
    """Stub d'ort.InferenceSession : logits cannés (numpy), MÊMES valeurs que
    le test torch (labels 2 = LOC pour les tokens 1-2)."""

    def __init__(self, path, providers=None):  # noqa: ARG002
        self._path = path

    def get_inputs(self):
        return [
            types.SimpleNamespace(name="input_ids"),
            types.SimpleNamespace(name="attention_mask"),
        ]

    def run(self, outputs, feeds):  # noqa: ARG002
        import numpy as np

        logits = np.array(
            [[[0.1, 0.1, 0.8], [0.1, 0.1, 0.8], [0.1, 0.1, 0.8], [0.9, 0.05, 0.05]]],
            dtype=np.float32,
        )
        return [logits]


def _write_onnx_files(tmp_path, int8: bool = True) -> Path:
    """Pré-provisionne le dossier <model>-onnx (layout réaliste : model.onnx
    fp32 + [model-int8.onnx] + label_map.json)."""
    onnx_dir = tmp_path / "afroxlmr-onnx"
    onnx_dir.mkdir(exist_ok=True)
    (onnx_dir / "model.onnx").write_bytes(b"fake-onnx-fp32")
    if int8:
        (onnx_dir / "model-int8.onnx").write_bytes(b"fake-onnx-int8")
    (onnx_dir / "label_map.json").write_text(
        '{"0": "O", "1": "PER", "2": "LOC"}', encoding="utf-8"
    )
    return onnx_dir


def test_onnx_backend_init_loads_session_and_labels(tmp_path, monkeypatch):
    """CLOISON_ONNX=1 : le backend ONNX charge la session + le label_map."""
    _write_onnx_files(tmp_path, int8=True)
    monkeypatch.setattr(
        african_mod, "ort", types.SimpleNamespace(InferenceSession=FakeOrtSession)
    )
    cfg = Config(model_dir=str(tmp_path), onnx=True, african=AfricanConfig(model_name="afroxlmr"))
    det = african_mod.AfricanModelDetector(cfg)
    assert det._try_onnx_backend("dummy-model", FakeTokenizerNp(), {}) is True
    assert det._session is not None
    assert det._backend() == "onnx-int8"
    assert det._labels_map == {0: "O", 1: "PER", 2: "LOC"}
    assert det._tokenizer is not None


def test_onnx_fp32_selected_when_int8_off(tmp_path, monkeypatch):
    """CLOISON_ONNX_INT8=0 : le fichier fp32 (model.onnx) est utilisé."""
    _write_onnx_files(tmp_path, int8=False)
    monkeypatch.setattr(
        african_mod, "ort", types.SimpleNamespace(InferenceSession=FakeOrtSession)
    )
    cfg = Config(
        model_dir=str(tmp_path), onnx=True, onnx_int8=False,
        african=AfricanConfig(model_name="afroxlmr"),
    )
    det = african_mod.AfricanModelDetector(cfg)
    assert det._try_onnx_backend("dummy-model", FakeTokenizerNp(), {}) is True
    assert det._backend() == "onnx"


def test_onnx_fallback_when_session_fails(tmp_path, monkeypatch):
    """Échec d'initialisation ONNX -> repli torch (jamais d'erreur, jamais de blocage)."""
    _write_onnx_files(tmp_path, int8=True)

    class BrokenSession(FakeOrtSession):
        def __init__(self, path, providers=None):  # noqa: ARG002
            raise RuntimeError("session cassée")

    monkeypatch.setattr(
        african_mod, "ort", types.SimpleNamespace(InferenceSession=BrokenSession)
    )
    cfg = Config(model_dir=str(tmp_path), onnx=True, african=AfricanConfig(model_name="afroxlmr"))
    det = african_mod.AfricanModelDetector(cfg)
    assert det._try_onnx_backend("dummy-model", FakeTokenizerNp(), {}) is False
    assert det._session is None


def test_onnx_detect_aligns_spans_from_numpy_logits(tmp_path, monkeypatch):
    """Le backend ONNX produit les mêmes spans que torch (mêmes logits cannés)."""
    _write_onnx_files(tmp_path, int8=True)
    monkeypatch.setattr(
        african_mod, "ort", types.SimpleNamespace(InferenceSession=FakeOrtSession)
    )
    det = african_mod.AfricanModelDetector(
        Config(model_dir=str(tmp_path), onnx=True, african=AfricanConfig(model_name="afroxlmr"))
    )
    assert det._try_onnx_backend("dummy-model", FakeTokenizerNp(), {}) is True
    det._load_attempted = True  # contourne le chargement réel
    spans = det.detect("Bonjour Aminata Diop à Dakar", locale="fr", policy=None)
    assert spans, "le backend ONNX doit produire des spans"
    assert spans[0].type is SpanType.LOC
    assert spans[0].score >= 0.50, "score au-dessus du seuil interne african"


def test_onnx_export_missing_file_degrades_gracefully(tmp_path, monkeypatch):
    """Fichier ONNX absent + export impossible -> False, pas de crash."""
    monkeypatch.setattr(
        african_mod, "ort", types.SimpleNamespace(InferenceSession=FakeOrtSession)
    )

    def boom(*a, **k):  # noqa: ARG002
        raise RuntimeError("modèle indisponible")

    monkeypatch.setattr(
        african_mod.AutoModelForTokenClassification, "from_pretrained", staticmethod(boom)
    )
    cfg = Config(model_dir=str(tmp_path), onnx=True, african=AfricanConfig(model_name="afroxlmr"))
    det = african_mod.AfricanModelDetector(cfg)
    assert det._try_onnx_backend("dummy-model", FakeTokenizerNp(), {}) is False
    assert det._session is None


def test_onnx_disabled_uses_torch_backend_by_default():
    """CLOISON_ONNX absent (défaut) : aucun backend ONNX sélectionné."""
    det = african_mod.AfricanModelDetector(Config(onnx=False))
    assert det._session is None
    assert det._backend() == "none"
