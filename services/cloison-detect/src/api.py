"""Transports REST (FastAPI) et gRPC, miroirs exacts du contrat proto.

REST : POST /detect (JSON), GET /healthz, GET /version, GET /models.
gRPC : DetectService (proto/detect.proto) — code généré requis (voir
README.md, `make proto`) ; sans lui, seul le REST est servi (dégradation).

Erreurs : {"error": {"code": ..., "message": ...}} — jamais le texte d'entrée
dans un message d'erreur. Aucune doc auto exposant du texte (docs désactivées).
"""

from __future__ import annotations

import logging
from typing import Any

from pydantic import BaseModel, ConfigDict, field_validator

from .detect_service import DetectRequest, DetectResponse, DetectService
from .spans import CanonicalMention, Policy, SessionContext, Span, SpanType

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Schémas REST (pydantic) — miroir du proto, champs camelCase
# ---------------------------------------------------------------------------


class RestSpan(BaseModel):
    model_config = ConfigDict(extra="forbid")

    start: int
    end: int
    type: str
    score: float

    @field_validator("score")
    @classmethod
    def _check_score(cls, v: float) -> float:
        if not (0.0 <= v <= 1.0):
            raise ValueError("score hors [0,1]")
        return v

    @field_validator("start", "end")
    @classmethod
    def _check_non_negative(cls, v: int) -> int:
        if v < 0:
            raise ValueError("offset négatif")
        return v


class RestPolicy(BaseModel):
    model_config = ConfigDict(extra="forbid")

    types: list[str] = []
    min_score: float = 0.40
    thresholds: dict[str, float] = {}
    mode: str = "balanced"
    enable_alias_expansion: bool = True
    enable_quasiid_gauge: bool = False
    models: list[str] = []
    quasiid_threshold: float | None = None

    @field_validator("min_score", "quasiid_threshold")
    @classmethod
    def _check_in_unit(cls, v: float | None) -> float | None:
        if v is not None and not (0.0 <= v <= 1.0):
            raise ValueError("seuil hors [0,1]")
        return v

    @field_validator("mode")
    @classmethod
    def _check_mode(cls, v: str) -> str:
        if v not in ("balanced", "high_precision", "recall_only"):
            raise ValueError(f"mode inconnu: {v!r}")
        return v


class RestMention(BaseModel):
    model_config = ConfigDict(extra="forbid")

    key: str
    type: str = "PERSON"
    locale: str = "fr"
    seen_count: int = 1

    @field_validator("seen_count")
    @classmethod
    def _check_seen(cls, v: int) -> int:
        if v < 1:
            raise ValueError("seen_count < 1")
        return v


class RestSession(BaseModel):
    model_config = ConfigDict(extra="forbid")

    mentions: list[RestMention] = []


class RestRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    text: str
    locale: str = "fr"
    policy: RestPolicy | None = None
    session: RestSession | None = None
    core_spans: list[RestSpan] = []


# ---------------------------------------------------------------------------
# Convertisseurs REST <-> interne
# ---------------------------------------------------------------------------


def _span_from_rest(s: RestSpan) -> Span:
    return Span(
        start=s.start, end=s.end, type=SpanType.parse(s.type),
        score=s.score, source="core",
    )


def _check_core_offsets(text: str, spans: tuple[Span, ...]) -> None:
    n = len(text)
    for s in spans:
        if not (0 <= s.start < s.end <= n):
            raise ValueError("offsets core_spans invalides")


def request_from_rest(payload: RestRequest) -> DetectRequest:
    policy = Policy.from_dict(payload.policy.model_dump() if payload.policy else None)
    session = SessionContext.from_dict(payload.session.model_dump() if payload.session else None)
    core = tuple(_span_from_rest(s) for s in payload.core_spans)
    _check_core_offsets(payload.text, core)
    return DetectRequest(
        text=payload.text, locale=payload.locale,
        policy=policy, session=session, core_spans=core,
    )


def response_to_rest(res: DetectResponse) -> dict[str, Any]:
    """Réponse JSON — miroir du proto : seuls start/end/type/score sortent."""
    payload: dict[str, Any] = {
        "spans": [
            {
                "start": s.start,
                "end": s.end,
                "type": s.type.value,
                "score": round(s.score, 4),
            }
            for s in res.spans
        ]
    }
    if res.quasi_id is not None:
        payload["quasi_id"] = res.quasi_id.to_dict()
    return payload


# ---------------------------------------------------------------------------
# Application FastAPI
# ---------------------------------------------------------------------------


def create_app(service: DetectService):
    """Construit l'application FastAPI (REST). Pas de docs auto (aucun texte)."""
    from fastapi import FastAPI, HTTPException, Request
    from fastapi.exceptions import RequestValidationError
    from fastapi.responses import JSONResponse

    app = FastAPI(
        title="cloison-detect",
        version=service.config.version,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )

    @app.exception_handler(RequestValidationError)
    async def validation_error_handler(request: Request, exc: RequestValidationError) -> JSONResponse:
        # JAMAIS le corps de requête dans l'erreur (fuite PII potentielle).
        # On ne renvoie que le code et un message générique.
        return JSONResponse(
            status_code=400,
            content={"error": {"code": "INVALID_ARGUMENT",
                               "message": "requête invalide (validation)"}},
        )

    @app.exception_handler(HTTPException)
    async def http_exception_handler(request: Request, exc: HTTPException) -> JSONResponse:
        # Normalise au format {"error": {...}} — sans jamais exposer de secret.
        detail = exc.detail
        if isinstance(detail, dict) and "error" in detail:
            body = detail
        else:
            body = {"error": {"code": "ERROR", "message": str(detail)}}
        return JSONResponse(status_code=exc.status_code, content=body)

    @app.post("/detect")
    def detect(payload: RestRequest) -> dict[str, Any]:
        # modèle lourd explicitement demandé mais indisponible -> 503
        requested = set(payload.policy.models) if payload.policy else set()
        status = service.model_status()
        unavailable = {m for m in requested if m not in status or not status[m]["available"]}
        if unavailable:
            raise HTTPException(
                status_code=503,
                detail={"error": {"code": "FAILED_PRECONDITION",
                                  "message": "modèle demandé indisponible"}},
            )
        try:
            request = request_from_rest(payload)
            response = service.detect(request)
        except ValueError as exc:
            raise HTTPException(
                status_code=400,
                detail={"error": {"code": "INVALID_ARGUMENT", "message": str(exc)}},
            ) from exc
        return response_to_rest(response)

    @app.get("/healthz")
    def healthz() -> dict[str, Any]:
        status = service.model_status()
        loaded = sorted(k for k, v in status.items() if v["loaded"])
        pending = sorted(k for k, v in status.items() if not v["loaded"] and v["available"])
        return {"status": "ok", "models_loaded": loaded, "models_pending": pending}

    @app.get("/version")
    def version() -> dict[str, str]:
        return {
            "name": "cloison-detect",
            "version": service.config.version,
            "proto": "cloison.detect.v1",
        }

    @app.get("/models")
    def models() -> dict[str, Any]:
        return {"models": service.model_status()}

    return app


# ---------------------------------------------------------------------------
# gRPC (code généré requis ; sans lui, le transport est désactivé)
# ---------------------------------------------------------------------------


_GEN_CACHE = None
_GEN_ATTEMPTED = False


def _load_proto_gen():
    """Importe le code généré (src/gen/detect_pb2*). None si absent (une seule tentative)."""
    global _GEN_CACHE, _GEN_ATTEMPTED
    if _GEN_ATTEMPTED:
        return _GEN_CACHE
    _GEN_ATTEMPTED = True
    try:
        from . import gen  # noqa: F401 — s'assure que src/gen est importable
        from .gen import detect_pb2, detect_pb2_grpc  # type: ignore[import-not-found]
        _GEN_CACHE = (detect_pb2, detect_pb2_grpc)
    except Exception as exc:
        logger.warning("grpc: code généré absent (%s) — transport gRPC désactivé", exc)
        _GEN_CACHE = None
    return _GEN_CACHE


def grpc_available() -> bool:
    """True si le code protobuf généré est présent (transport gRPC possible)."""
    return _load_proto_gen() is not None


def _policy_from_proto(p) -> Policy:
    qid = p.quasiid_threshold if p.HasField("quasiid_threshold") else None
    return Policy(
        types=frozenset(SpanType.parse(t) for t in p.types),
        min_score=p.min_score if p.min_score != 0.0 else 0.40,
        thresholds={SpanType.parse(k): v for k, v in p.thresholds.items()},
        mode=p.mode or "balanced",
        models=tuple(p.models),
        enable_alias_expansion=p.enable_alias_expansion
        if p.HasField("enable_alias_expansion") else True,
        enable_quasiid_gauge=p.enable_quasiid_gauge
        if p.HasField("enable_quasiid_gauge") else False,
        quasiid_threshold=float(qid) if qid is not None else 0.50,
    )


def _session_from_proto(s) -> SessionContext:
    mentions = tuple(
        CanonicalMention(
            key=m.key,
            type=SpanType.parse(m.type),
            locale=m.locale or "fr",
            seen_count=max(1, int(m.seen_count)),
        )
        for m in s.mentions
    )
    return SessionContext(mentions=mentions)


def _span_from_proto(s) -> Span:
    return Span(
        start=int(s.start), end=int(s.end),
        type=SpanType.parse(s.type), score=float(s.score), source="core",
    )


def request_from_proto(msg) -> DetectRequest:
    text = msg.text
    policy = _policy_from_proto(msg.policy) if msg.HasField("policy") else Policy()
    session = _session_from_proto(msg.session) if msg.HasField("session") else SessionContext()
    core = tuple(_span_from_proto(s) for s in msg.core_spans)
    _check_core_offsets(text, core)
    return DetectRequest(text=text, locale=msg.locale or "fr",
                         policy=policy, session=session, core_spans=core)


def response_to_proto(res: DetectResponse, pb2):
    out = pb2.DetectResponse()
    for s in res.spans:
        sp = out.spans.add()
        sp.start, sp.end, sp.type, sp.score = s.start, s.end, s.type.value, float(s.score)
    if res.quasi_id is not None:
        out.quasi_id.score = res.quasi_id.score
        out.quasi_id.flagged = res.quasi_id.flagged
        out.quasi_id.signals.extend(res.quasi_id.signals)
    return out


def make_servicer(service: DetectService, pb2, pb2_grpc):
    """Fabrique le servicer gRPC lié au pipeline (imports grpc lazy)."""
    import grpc

    class DetectServicer(pb2_grpc.DetectServiceServicer):
        def Detect(self, request, context):  # noqa: N802 — API gRPC imposée
            try:
                req = request_from_proto(request)
                res = service.detect(req)
            except ValueError as exc:
                context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
                context.set_details(str(exc))
                return pb2.DetectResponse()
            # deadline douce atteinte ? pas d'erreur : le rappel est un bonus
            return response_to_proto(res, pb2)

    return DetectServicer()


def serve_grpc(service: DetectService, port: int) -> None:
    """Démarre le serveur gRPC (bloquant). Lève si le code généré est absent."""
    import grpc
    from concurrent import futures

    gen = _load_proto_gen()
    if gen is None:
        raise RuntimeError("code gRPC généré absent — lancez `make proto`")
    pb2, pb2_grpc = gen
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=8))
    pb2_grpc.add_DetectServiceServicer_to_server(make_servicer(service, pb2, pb2_grpc), server)
    server.add_insecure_port(f"[::]:{port}")
    server.start()
    logger.info("grpc: DetectService à l'écoute sur [::]:%s", port)
    server.wait_for_termination()
