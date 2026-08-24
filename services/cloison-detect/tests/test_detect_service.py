"""Tests du pipeline complet (DetectService) avec DÉTECTEURS STUB.

Aucun téléchargement, aucun gros modèle : les détecteurs sont remplacés par
des stubs (monkeypatch des attributs internes) ; le code de fusion, d'alias,
de jauge et de transport est testé tel quel. La dégradation gracieuse est
vérifiée (GLiNER indisponible ou en crash -> le pipeline continue).
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pytest

from src.config import Config
from src.detect_service import DetectRequest, DetectService
from src.spans import CanonicalMention, Policy, SessionContext, Span, SpanType

TEXT = "Marie Dupont habite à Ouagadougou."
TEXT_PERSON_LOC = "Marie Dupont habite à Ouagadougou."


class StubPresidio:
    """Stub de PresidioOracle : spans fixes, aucun chargement."""

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
    """Stub de GlinerDetector : spans fixes ou indisponible/crash."""

    name = "gliner"

    def __init__(self, spans=None, available=True, raise_on_detect=False):
        self.spans = spans or []
        self.available_flag = available
        self.raise_on_detect = raise_on_detect
        self.calls = 0

    def detect(self, text, locale="fr", policy=None):
        self.calls += 1
        if self.raise_on_detect:
            raise RuntimeError("modèle corrompu")
        return list(self.spans)

    def available(self):
        return self.available_flag

    def loaded(self):
        return self.available_flag


class StubAfrican:
    """Stub du NER ouest-africain : muet par défaut (les tests unitaires ne
    doivent pas charger de vrais modèles)."""

    name = "afro"

    def __init__(self, spans=None):
        self.spans = spans if spans is not None else []

    def detect(self, text, locale="fr", policy=None):
        return list(self.spans)

    def available(self):
        return False

    def loaded(self):
        return False

    supported_models = ("serengeti", "afroxlmr", "masakha")

    def status(self):
        return {"loaded": False, "available": False, "model": "stub", "model_id": None}


def make_service(presidio=None, gliner=None, african=None, **config_over) -> DetectService:
    cfg = Config(**config_over)
    svc = DetectService(cfg)
    svc._presidio = presidio if presidio is not None else StubPresidio()
    svc._gliner = gliner if gliner is not None else StubGliner()
    svc._african = african if african is not None else StubAfrican()
    return svc


def person_span(start=0, end=12, score=0.93) -> Span:
    return Span(start, end, SpanType.PERSON, score, source="presidio")


def loc_span(start=22, end=33, score=0.92) -> Span:
    return Span(start, end, SpanType.LOC, score, source="presidio")


def test_detect_basic_fusion():
    svc = make_service(presidio=StubPresidio([person_span(), loc_span()]))
    req = DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr-BF",
        policy=Policy(types=frozenset({SpanType.PERSON, SpanType.LOC})),
    )
    res = svc.detect(req)
    by_type = {(s.type, s.start, s.end) for s in res.spans}
    assert (SpanType.PERSON, 0, 12) in by_type
    assert (SpanType.LOC, 22, 33) in by_type
    assert res.partial is False


def test_gliner_boosts_recall():
    presidio = StubPresidio([person_span()])
    gliner = StubGliner([Span(21, 33, SpanType.LOC, 0.9, source="gliner")])
    svc = make_service(presidio=presidio, gliner=gliner)
    req = DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON, SpanType.LOC})),
    )
    res = svc.detect(req)
    types = {s.type for s in res.spans}
    assert SpanType.PERSON in types and SpanType.LOC in types
    assert gliner.calls == 1


def test_gliner_not_called_for_unrelated_types():
    gliner = StubGliner()
    svc = make_service(presidio=StubPresidio([person_span()]), gliner=gliner)
    req = DetectRequest(text="x", locale="fr", policy=Policy(types=frozenset({SpanType.DATE})))
    res = svc.detect(req)
    assert gliner.calls == 0
    assert res.spans == ()   # PERSON non demandé -> filtré ; DATE non détecté


def test_graceful_degradation_gliner_crash():
    presidio = StubPresidio([person_span()])
    svc = make_service(presidio=presidio, gliner=StubGliner(raise_on_detect=True))
    req = DetectRequest(
        text=TEXT, locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON, SpanType.LOC})),
    )
    res = svc.detect(req)   # ne doit pas lever
    assert any(s.type is SpanType.PERSON for s in res.spans)


def test_gliner_unavailable_returns_empty():
    presidio = StubPresidio([person_span()])
    svc = make_service(presidio=presidio, gliner=StubGliner(available=False))
    req = DetectRequest(
        text=TEXT, locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON, SpanType.LOC})),
    )
    res = svc.detect(req)
    assert any(s.type is SpanType.PERSON for s in res.spans)


def test_empty_text_idempotent():
    svc = make_service()
    res = svc.detect(DetectRequest(text="", locale="fr"))
    assert res.spans == ()
    assert res.quasi_id is None


def test_core_spans_dedupe():
    svc = make_service(presidio=StubPresidio([person_span()]))
    core = (Span(0, 12, SpanType.PERSON, 1.0, source="core"),)
    req = DetectRequest(text=TEXT, locale="fr", policy=Policy(), core_spans=core)
    res = svc.detect(req)
    assert res.spans == ()   # le core fait foi : span sidecar dédupliqué


def test_alias_expansion_integration():
    presidio = StubPresidio([person_span()])
    svc = make_service(presidio=presidio)
    session = SessionContext(
        mentions=(CanonicalMention(key="Marie Dupont", type=SpanType.PERSON, seen_count=2),)
    )
    text = "Marie Dupont est partie. Marie reviendra."
    req = DetectRequest(text=text, locale="fr",
                        policy=Policy(enable_alias_expansion=True), session=session)
    res = svc.detect(req)
    alias = [s for s in res.spans if text[s.start:s.end] == "Marie" and s.type is SpanType.PERSON]
    assert alias, "l'alias « Marie » doit être détecté"
    assert alias[0].start >= 12


def test_alias_disabled():
    presidio = StubPresidio([person_span()])
    svc = make_service(presidio=presidio)
    session = SessionContext(
        mentions=(CanonicalMention(key="Marie Dupont", type=SpanType.PERSON),)
    )
    req = DetectRequest(text=TEXT, locale="fr",
                        policy=Policy(enable_alias_expansion=False), session=session)
    res = svc.detect(req)
    assert all(s.source != "alias" for s in res.spans)


def test_quasiid_integration():
    presidio = StubPresidio([person_span()])
    svc = make_service(presidio=presidio)
    text = "Marie a 42 ans. Acte n° 1847. Le 12/03/2021. Ouagadougou."
    idx = text.find("Ouagadougou")
    core = (Span(idx, idx + len("Ouagadougou"), SpanType.LOC, 1.0, source="core"),)
    req = DetectRequest(text=text, locale="fr",
                        policy=Policy(enable_quasiid_gauge=True), core_spans=core)
    res = svc.detect(req)
    assert res.quasi_id is not None
    assert set(res.quasi_id.signals) >= {"age", "act", "date", "loc"}
    # désactivée par défaut
    res2 = svc.detect(DetectRequest(text=text, locale="fr", policy=Policy(), core_spans=core))
    assert res2.quasi_id is None


def test_invalid_core_offsets_rejected():
    svc = make_service()
    core = (Span(0, 500, SpanType.PERSON, 1.0, source="core"),)
    req = DetectRequest(text="court texte", locale="fr", policy=Policy(), core_spans=core)
    with pytest.raises(ValueError):
        svc.detect(req)


def test_invalid_locale_rejected():
    svc = make_service()
    with pytest.raises(ValueError):
        svc.detect(DetectRequest(text="x", locale="fr!!", policy=Policy()))


def test_high_precision_requires_presidio():
    gliner = StubGliner([Span(0, 12, SpanType.PERSON, 0.9, source="gliner")])
    svc = make_service(presidio=StubPresidio([]), gliner=gliner)
    req = DetectRequest(
        text=TEXT, locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON}), mode="high_precision"),
    )
    res = svc.detect(req)
    assert all(s.type is not SpanType.PERSON for s in res.spans)  # pas de consensus Presidio


def test_recall_only_lowers_threshold():
    presidio = StubPresidio([Span(0, 12, SpanType.PERSON, 0.35, source="presidio")])
    svc = make_service(presidio=presidio)
    # Mode par defaut = "balanced" (facteur 1.0) : un span sous min_score (0.40)
    # est filtre.
    balanced = dict(text=TEXT, locale="fr",
                    policy=Policy(types=frozenset({SpanType.PERSON}), mode="balanced"))
    assert not any(s.type is SpanType.PERSON for s in svc.detect(DetectRequest(**balanced)).spans)
    # recall_only abaisse le seuil (facteur 0.85 : 0.40 x 0.85 = 0.34 <= 0.35) :
    # le span passe.
    recall = dict(text=TEXT, locale="fr",
                  policy=Policy(types=frozenset({SpanType.PERSON}), mode="recall_only"))
    assert any(s.type is SpanType.PERSON for s in svc.detect(DetectRequest(**recall)).spans)


def test_budget_exhausted_partial():
    svc = make_service(budget_seconds=0.0)
    res = svc.detect(DetectRequest(text="texte", locale="fr"))
    assert res.partial is True
    assert res.spans == ()


def test_deterministic_order_and_no_overlap():
    presidio = StubPresidio([
        Span(0, 12, SpanType.PERSON, 0.93, source="presidio"),
        Span(6, 12, SpanType.PERSON, 0.80, source="presidio"),   # sous-span -> fusion
        Span(21, 33, SpanType.LOC, 0.88, source="presidio"),
    ])
    svc = make_service(presidio=presidio)
    req = DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr",
        policy=Policy(types=frozenset({SpanType.PERSON, SpanType.LOC})),
    )
    res = svc.detect(req)
    spans = res.spans
    for a, b in zip(spans, spans[1:]):
        assert a.start <= b.start
        assert a.end <= b.start or a.start >= b.end   # non-chevauchement
    person = [s for s in spans if s.type is SpanType.PERSON]
    assert len(person) == 1
    assert (person[0].start, person[0].end) == (0, 12)
    assert person[0].score == pytest.approx((0.93 + 0.80) / 2, abs=1e-4)  # vote pondéré


# ---------------------------------------------------------------------------
# Contrat REST (smoke, stubs) — la même requête par les deux transports
# ---------------------------------------------------------------------------


def test_rest_contract_smoke():
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from src.api import create_app

    svc = make_service(presidio=StubPresidio([person_span(), loc_span()]))
    client = TestClient(create_app(svc))
    payload = {
        "text": TEXT_PERSON_LOC,
        "locale": "fr",
        "policy": {"types": ["PERSON", "LOC"], "min_score": 0.4,
                   "enable_alias_expansion": True, "enable_quasiid_gauge": False},
        "session": {"mentions": [{"key": "Marie Dupont", "type": "PERSON",
                                  "locale": "fr", "seen_count": 2}]},
        "core_spans": [],
    }
    r = client.post("/detect", json=payload)
    assert r.status_code == 200
    body = r.json()
    assert body["spans"]
    assert {s["type"] for s in body["spans"]} <= {"PERSON", "LOC"}
    for s in body["spans"]:
        assert 0 <= s["start"] < s["end"] <= len(TEXT_PERSON_LOC)
    assert client.get("/healthz").json()["status"] == "ok"
    assert client.get("/version").json()["proto"] == "cloison.detect.v1"
    assert client.get("/models").json()["models"]["presidio"]["loaded"] is True


def test_rest_invalid_offsets_400():
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from src.api import create_app

    client = TestClient(create_app(make_service()))
    payload = {"text": "abc", "locale": "fr",
               "core_spans": [{"start": 0, "end": 99, "type": "PERSON", "score": 1.0}]}
    r = client.post("/detect", json=payload)
    assert r.status_code == 400
    assert r.json()["error"]["code"] == "INVALID_ARGUMENT"


def test_rest_requested_model_unavailable_503():
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from src.api import create_app

    svc = make_service(gliner=StubGliner(available=False))
    client = TestClient(create_app(svc))
    payload = {"text": "abc", "locale": "fr", "policy": {"models": ["gliner"]}}
    r = client.post("/detect", json=payload)
    assert r.status_code == 503
    assert r.json()["error"]["code"] == "FAILED_PRECONDITION"
def test_consensus_rejects_mono_source_low_score():
    """Consensus PERSON/LOC (défaut ON) : un span mono-source < 0.90 est refusé."""
    from src.spans import Policy, SpanType, Span
    svc = make_service(presidio=StubPresidio([person_span(score=0.85)]))
    resp = svc.detect(DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr", policy=Policy(types=frozenset({SpanType.PERSON})),
        core_spans=(), session=SessionContext()))
    assert not any(s.type is SpanType.PERSON for s in resp.spans), \
        "mono-source < 0.90 doit être refusé (spécificité)"


def test_consensus_keeps_multi_source_and_high_single():
    """Un span 2 sources passe ; un mono-source >= 0.90 passe aussi."""
    from src.spans import Policy, SpanType, Span
    g1 = StubGliner()
    g1.spans = [Span(0, 12, SpanType.PERSON, 0.93, source="gliner")]
    svc = make_service(
        presidio=StubPresidio([person_span(start=0, end=12, score=0.85)]),
        gliner=g1,
    )
    resp = svc.detect(DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr", policy=Policy(types=frozenset({SpanType.PERSON})),
        core_spans=(), session=SessionContext()))
    assert any(s.type is SpanType.PERSON for s in resp.spans), "consensus 2 sources gardé"


def test_consensus_can_be_disabled():
    """CLOISON_CONSENSUS_PERSON_LOC=0 : le mono-source repasse."""
    svc = make_service(presidio=StubPresidio([person_span(score=0.85)]),
                       consensus_person_loc=False)
    resp = svc.detect(DetectRequest(
        text=TEXT_PERSON_LOC, locale="fr", policy=Policy(types=frozenset({SpanType.PERSON})),
        core_spans=(), session=SessionContext()))
    assert any(s.type is SpanType.PERSON for s in resp.spans), "consensus désactivé → mono-source admis"


def test_concurrency_gate_limits_parallel_pipelines():
    """CLOISON_DETECT_CONCURRENCY=1 : les pipelines sont bornés (sémaphore).
    Vérifie que 2 requêtes simultanées sont sérialisées — le compteur de
    pipelines actifs ne dépasse jamais 1."""
    import threading
    import time as _time

    active = 0
    peak = 0
    lock = threading.Lock()

    class SlowStub:
        name = "presidio"

        def detect(self, text, locale="fr", policy=None):
            nonlocal active, peak
            with lock:
                active += 1
                peak = max(peak, active)
            _time.sleep(0.05)
            with lock:
                active -= 1
            return [person_span()]

        def available(self):
            return True

        def presidio_loaded(self):
            return True

    svc = make_service(presidio=SlowStub(), concurrency=1)
    results = []
    errors = []

    def run():
        try:
            results.append(svc.detect(DetectRequest(
                text=TEXT_PERSON_LOC, locale="fr",
                policy=Policy(types=frozenset({SpanType.PERSON})),
            )))
        except Exception as exc:  # pragma: no cover
            errors.append(exc)

    threads = [threading.Thread(target=run) for _ in range(3)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert not errors, errors
    assert len(results) == 3, "les 3 requêtes aboutissent"
    assert peak <= 1, f"concurrence>1 avec gate=1 (peak={peak})"

    # Concurrence illimitée (défaut 0) : pas de gate.
    svc_unlimited = make_service(presidio=SlowStub(), concurrency=0)
    assert svc_unlimited._gate is None


def test_concurrency_from_env():
    """Config.from_env lit CLOISON_DETECT_CONCURRENCY (0 = illimité)."""
    cfg = Config.from_env({"CLOISON_DETECT_CONCURRENCY": "3"})
    assert cfg.concurrency == 3
    cfg2 = Config.from_env({})
    assert cfg2.concurrency == 0
