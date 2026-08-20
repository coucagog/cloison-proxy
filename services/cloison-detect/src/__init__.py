"""cloison-detect — sidecar de détection NER lourd (STACK-6).

Le sidecar ne fait QUE détecter : il ne tokenise jamais, il ne pseudonymise
jamais, il ne résout jamais d'identité. Le core Rust (STACK-2/4) reste la
source de vérité de la tokenisation et des décisions ; il consomme les spans
renvoyés par ce service via gRPC (proto/detect.proto) avec repli REST/JSON.
"""

from __future__ import annotations

__version__ = "0.1.0"
__proto_package__ = "cloison.detect.v1"

__all__ = ["__version__", "__proto_package__"]
