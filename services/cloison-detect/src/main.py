"""Point d'entrée du sidecar : lance uvicorn (REST) et/ou le serveur gRPC.

Usage :
    python -m src.main                     # REST (uvicorn) sur CLOISON_REST_PORT
    CLOISON_TRANSPORT=both python -m src.main
    python -m src.main --check             # précharge (niveau env) puis exit 0
    python -m src.main --transport grpc --port 50051

Le transport nominal (production) est gRPC ; le REST est le repli (et le
défaut local). Sans code protobuf généré, le gRPC est désactivé avec un
avertissement et le REST reste servi.
"""

from __future__ import annotations

import argparse
import logging
import sys
from threading import Thread

from .api import create_app, grpc_available, serve_grpc
from .config import Config
from .detect_service import DetectService

logger = logging.getLogger(__name__)


def _configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    parser = argparse.ArgumentParser(prog="cloison-detect")
    parser.add_argument("--check", action="store_true",
                        help="précharge les modèles (niveau env) puis exit 0")
    parser.add_argument("--transport", choices=("grpc", "rest", "both"), default=None)
    parser.add_argument("--port", type=int, default=None)
    opts, _ = parser.parse_known_args(args)

    config = Config.from_env()
    transport = opts.transport or config.transport
    if opts.port is not None:
        config = config.model_copy(update={"rest_port": opts.port, "grpc_port": opts.port})
    _configure_logging(config.log_level)

    service = DetectService(config)
    if opts.check:
        service.preload(config.preload)
        logger.info("check: configuration OK (transport=%s, preload=%s)", transport, config.preload)
        return 0

    # B.2 — préchargement au boot (CLOISON_PRELOAD != none) : les modèles sont
    # chargés AVANT de servir (latence du 1er appel évitée, pic mémoire
    # maîtrisé au démarrage). Dégradation gracieuse : un modèle absent ou un
    # téléchargement HF qui échoue ne fait jamais tomber le service.
    if config.preload != "none":
        logger.info("preload au boot: niveau=%s (chargement des modèles...)", config.preload)
        service.preload(config.preload)
        logger.info("preload au boot: terminé (niveau=%s)", config.preload)
    else:
        logger.info("preload au boot: désactivé (CLOISON_PRELOAD=none) — chargement lazy")

    if transport == "grpc":
        if not grpc_available():
            logger.error("grpc: code généré absent — impossible de servir le transport nominal")
            return 2
        serve_grpc(service, config.grpc_port)  # bloquant
        return 0

    if transport == "both":
        if grpc_available():
            thread = Thread(target=serve_grpc, args=(service, config.grpc_port), daemon=True)
            thread.start()
        else:
            logger.warning("grpc: code généré absent — REST seul")

    # rest (défaut) ou both : uvicorn porte le repli REST
    import uvicorn

    app = create_app(service)
    logger.info("rest: uvicorn sur 0.0.0.0:%s", config.rest_port)
    uvicorn.run(app, host="0.0.0.0", port=config.rest_port,
                log_level=config.log_level.lower())
    return 0


if __name__ == "__main__":
    sys.exit(main())
