"""Types canoniques du sidecar : spans, politique, session — sérialisation JSON.

Miroir Python du contrat protobuf (proto/detect.proto) et des schémas REST
(src/api.py). Aucune dépendance externe : ce module est importable partout,
y compris dans les tests (pas de réseau, pas de modèles).
"""

from __future__ import annotations

import json
import re
import unicodedata
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Mapping

# ---------------------------------------------------------------------------
# Utilitaires texte (partagés : alias, gazetteers, index)
# ---------------------------------------------------------------------------

_ACCENTED_BY_BASE: dict[str, tuple[str, ...]] = {
    "a": ("à", "â", "ä", "á", "ã", "å"),
    "c": ("ç",),
    "e": ("é", "è", "ê", "ë"),
    "i": ("î", "ï", "í", "ì"),
    "n": ("ñ",),
    "o": ("ô", "ö", "ò", "ó", "õ"),
    "u": ("ù", "û", "ü", "ú"),
    "y": ("ÿ",),
}

# Formes pronominales / mots-outils : JAMAIS traitées comme des fuites
# d'identité (l'expandeur d'alias refuse de les dériver ou de les matcher).
PRONOUN_FORMS: frozenset[str] = frozenset({
    "il", "elle", "ils", "elles", "on", "nous", "vous", "tu", "te", "toi",
    "je", "j", "moi", "lui", "leur", "leurs", "le", "la", "les", "y", "en",
    "ce", "cet", "cette", "ces", "un", "une", "des", "du", "de", "d", "l",
    "qui", "que", "quoi", "dont", "ou", "où",
})


def normalize_text(value: str) -> str:
    """Normalise casse + diacritiques (règle R7) : minuscules, accents retirés.

    « OUAGADOUGOU » / « Ouagadougou » / « Oùagàdougou » → « ouagadougou ».
    N'affecte jamais les offsets : utilisée pour l'index d'alias et les
    gazetteers, jamais pour produire des spans.
    """
    decomposed = unicodedata.normalize("NFKD", value)
    flat = "".join(ch for ch in decomposed if not unicodedata.combining(ch))
    return flat.casefold()


def insensitive_pattern(word: str) -> str:
    """Construit une classe regex insensible casse + diacritiques pour un mot.

    « Aïcha » → [AaÀàÂâÄáãå][IiÎîÏÍÌíìï]... — réutilisé par les gazetteers
    (presidio_oracle) et l'index d'alias (alias.AliasExpander).
    """
    out: list[str] = []
    for ch in word:
        if not ch.isalnum():
            out.append(re.escape(ch))
            continue
        folded = unicodedata.normalize("NFKD", ch)
        base = "".join(c for c in folded if not unicodedata.combining(c)) or ch
        variants = {base, base.lower(), base.upper()}
        for extra in _ACCENTED_BY_BASE.get(base.lower(), ()):
            variants.update((extra, extra.upper()))
        if len(variants) > 1:
            out.append("[" + "".join(sorted(variants)) + "]")
        else:
            out.append(re.escape(base))
    return "".join(out)


# ---------------------------------------------------------------------------
# Types de spans
# ---------------------------------------------------------------------------


class SpanType(str, Enum):
    """Type d'un span au niveau wire (string) ; valeurs inconnues → UNKNOWN,
    jamais d'exception de parsing."""

    PERSON = "PERSON"
    LOC = "LOC"
    ORG = "ORG"
    DATE = "DATE"
    AGE = "AGE"
    ACT = "ACT"
    ID = "ID"
    UNKNOWN = "UNKNOWN"  # valeur wire inconnue, jamais d'exception

    @classmethod
    def parse(cls, value: str) -> "SpanType":
        try:
            return cls(str(value).upper())
        except (ValueError, TypeError):
            return cls.UNKNOWN


@dataclass(frozen=True, slots=True)
class Span:
    """Un span détecté, offsets caractères relatifs au texte d'origine.

    Contract : 0 <= start < end <= len(text) ; start/end ne coupent jamais un
    point de code UTF-8 (les offsets Python sont des points de code).
    """

    start: int
    end: int
    type: SpanType
    score: float
    source: str = "unknown"       # "presidio" | "gliner" | "alias" | "ensemble" | ...
    alias_of: str | None = None   # key canonique si source == "alias"

    def __post_init__(self) -> None:
        if not isinstance(self.start, int) or not isinstance(self.end, int):
            raise ValueError(f"bornes non entières: ({self.start!r}, {self.end!r})")
        if not (0 <= self.start < self.end):
            raise ValueError(f"bornes invalides: ({self.start}, {self.end})")
        if not isinstance(self.score, (int, float)) or not (0.0 <= self.score <= 1.0):
            raise ValueError(f"score hors bornes: {self.score!r}")

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "start": self.start,
            "end": self.end,
            "type": self.type.value,
            "score": self.score,
        }
        if self.source != "unknown":
            data["source"] = self.source
        if self.alias_of is not None:
            data["alias_of"] = self.alias_of
        return data

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "Span":
        return cls(
            start=int(data["start"]),
            end=int(data["end"]),
            type=SpanType.parse(str(data.get("type", ""))),
            score=float(data["score"]),
            source=str(data.get("source", "unknown")),
            alias_of=data.get("alias_of"),
        )

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, sort_keys=True)

    @classmethod
    def from_json(cls, raw: str) -> "Span":
        return cls.from_dict(json.loads(raw))


def iou(a: Span, b: Span) -> float:
    """Intersection sur la plus petite des deux longueurs (IoS), dans [0,1].

    IoS(a, b) = |a ∩ b| / min(|a|, |b|), 0 si disjoint. Robuste aux
    sous-spans : « Marie » ⊂ « Marie Dupont » donne 1.0.
    """
    inter = min(a.end, b.end) - max(a.start, b.start)
    if inter <= 0:
        return 0.0
    return inter / min(a.end - a.start, b.end - b.start)


# ---------------------------------------------------------------------------
# Politique par requête (miroir de proto Policy)
# ---------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class Policy:
    """Politique appliquée à une détection (proto Policy)."""

    types: frozenset[SpanType] = frozenset()          # vide = tous
    min_score: float = 0.40
    thresholds: Mapping[SpanType, float] = field(default_factory=dict)
    mode: str = "balanced"                            # balanced | high_precision | recall_only
    models: tuple[str, ...] = ()                      # noms explicites de modèles
    enable_alias_expansion: bool = True
    enable_quasiid_gauge: bool = False
    quasiid_threshold: float = 0.50

    def __post_init__(self) -> None:
        if not (0.0 <= self.min_score <= 1.0):
            raise ValueError(f"min_score hors bornes: {self.min_score}")
        if self.mode not in ("balanced", "high_precision", "recall_only"):
            raise ValueError(f"mode inconnu: {self.mode!r}")
        if not (0.0 <= self.quasiid_threshold <= 1.0):
            raise ValueError(f"quasiid_threshold hors bornes: {self.quasiid_threshold}")
        for t, s in self.thresholds.items():
            if not (0.0 <= s <= 1.0):
                raise ValueError(f"seuil hors bornes pour {t.value}: {s}")

    def threshold_for(self, t: SpanType) -> float:
        return self.thresholds.get(t, self.min_score)

    def wants_type(self, t: SpanType) -> bool:
        return not self.types or t in self.types

    @classmethod
    def from_dict(cls, data: Mapping[str, Any] | None) -> "Policy":
        if not data:
            return cls()
        types = frozenset(SpanType.parse(str(x)) for x in data.get("types", []))
        thresholds = {
            SpanType.parse(str(k)): float(v)
            for k, v in (data.get("thresholds") or {}).items()
        }
        qid = data.get("quasiid_threshold")
        return cls(
            types=types,
            min_score=float(data.get("min_score", 0.40)),
            thresholds=thresholds,
            mode=str(data.get("mode", "balanced")),
            models=tuple(str(x) for x in data.get("models", [])),
            enable_alias_expansion=bool(data.get("enable_alias_expansion", True)),
            enable_quasiid_gauge=bool(data.get("enable_quasiid_gauge", False)),
            quasiid_threshold=float(qid) if qid is not None else 0.50,
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "types": sorted(t.value for t in self.types),
            "min_score": self.min_score,
            "thresholds": {t.value: s for t, s in self.thresholds.items()},
            "mode": self.mode,
            "models": list(self.models),
            "enable_alias_expansion": self.enable_alias_expansion,
            "enable_quasiid_gauge": self.enable_quasiid_gauge,
            "quasiid_threshold": self.quasiid_threshold,
        }


# ---------------------------------------------------------------------------
# Contexte de session (fourni par le core ; le sidecar est stateless)
# ---------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class CanonicalMention:
    """Mention canonique établie par le core dans la session (proto Mention)."""

    key: str
    type: SpanType
    locale: str = "fr"
    seen_count: int = 1

    def __post_init__(self) -> None:
        if not self.key or not self.key.strip():
            raise ValueError("mention canonique vide")
        if self.seen_count < 1:
            raise ValueError(f"seen_count invalide: {self.seen_count}")

    def to_dict(self) -> dict[str, Any]:
        return {
            "key": self.key,
            "type": self.type.value,
            "locale": self.locale,
            "seen_count": self.seen_count,
        }

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "CanonicalMention":
        return cls(
            key=str(data["key"]),
            type=SpanType.parse(str(data.get("type", "PERSON"))),
            locale=str(data.get("locale", "fr")),
            seen_count=int(data.get("seen_count", 1)),
        )


@dataclass(frozen=True, slots=True)
class SessionContext:
    """Contexte de session passé par le core (proto SessionContext)."""

    mentions: tuple[CanonicalMention, ...] = ()

    @classmethod
    def from_dict(cls, data: Mapping[str, Any] | None) -> "SessionContext":
        if not data:
            return cls()
        mentions = tuple(CanonicalMention.from_dict(m) for m in data.get("mentions", []))
        return cls(mentions=mentions)

    def to_dict(self) -> dict[str, Any]:
        return {"mentions": [m.to_dict() for m in self.mentions]}
