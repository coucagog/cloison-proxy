"""Configuration du sidecar (pydantic). Priorité : env CLOISON_* > défauts.

Jamais de secret, jamais de texte/PII dans cette configuration. Les modèles
lourds sont activés/désactivés ici (chargement lazy, dégradation gracieuse).
"""

from __future__ import annotations

import os
from typing import Mapping

from pydantic import BaseModel, Field, field_validator

from .spans import PRONOUN_FORMS

# ---------------------------------------------------------------------------
# Gazettes par défaut (petit jeu FR / ouest-africain ; le core peut en fournir
# de plus larges via la configuration). Le sidecar ne fait que matcher des
# mots entiers : pas d'inférence.
# ---------------------------------------------------------------------------

_DEFAULT_GAZETTEERS: dict[str, list[str]] = {
    "LOC": [
        "Ouagadougou", "Bobo-Dioulasso", "Koudougou", "Ouahigouya", "Banfora",
        "Kaya", "Dakar", "Abidjan", "Lomé", "Lome", "Cotonou", "Niamey",
        "Bamako", "N'Djamena", "Ndjamena", "Conakry", "Accra", "Lagos", "Kano",
        "Maradi", "Zinder", "Sikasso", "Ségou", "Segou", "Saint-Louis",
        "Thiès", "Thies", "Parakou", "Bohicon", "Kara", "Sokodé", "Sokode",
        "Atakpamé", "Atakpame",
    ],
    "PERSON": [
        "Marie", "Mamadou", "Aminata", "Fatou", "Ibrahim", "Awa", "Moussa",
        "Aïcha", "Aicha", "Ousmane", "Adama", "Seydou", "Kadiatou", "Issouf",
        "Ramata", "Salif", "Bintou", "Modibo", "Fanta", "Aboubacar",
        "Bakary", "Djénéba", "Djeneba", "Sékou", "Sekou", "Alassane",
    ],
}

_DEFAULT_COMMON_NAMES: frozenset[str] = frozenset({
    "les", "le", "la", "de", "du", "des", "et", "un", "une", "au", "aux",
    "madame", "monsieur", "mme", "mlle", "dr", "pr", "m", "a", "à", "ce",
    "cette", "ces", "son", "sa", "ses", "mon", "ma", "mes", "pour", "par",
    "sur", "dans", "avec", "sans", "chez", "entre", "sous", "vers",
    "depuis", "pendant", "avant", "après", "tous", "tout", "toute",
    "toutes", "chaque", "quelque", "autres", "autre", "rue", "avenue",
    "secteur", "quartier", "ville", "commune", "province", "région",
    "region", "departement", "département", "cité", "cite", "non", "oui",
    "bonjour", "merci", "ministère", "ministere", "préfecture", "prefecture",
    "mairie", "hôpital", "hopital", "école", "ecole", "marché", "marche",
    "place", "centre", "camp", "village", "terrain", "parcelle", "lot",
    "numero", "numéro", "n", "n°",
})


# ---------------------------------------------------------------------------
# Sous-configurations
# ---------------------------------------------------------------------------


class PresidioConfig(BaseModel):
    """Oracle de référence : analyzeur Presidio (lazy) + regex CNI + gazetteers."""

    enabled: bool = True
    languages: tuple[str, ...] = ("fr", "en")
    enable_cni: bool = True               # regex numéro CNI -> SpanType.ID
    cni_score: float = 0.95
    gazetteer_score: float = 0.85
    gazetteers: dict[str, list[str]] = Field(default_factory=lambda: dict(_DEFAULT_GAZETTEERS))


class GlinerConfig(BaseModel):
    """NER zéro-shot GLiNER (chargement lazy, dégradation si absent)."""

    enabled: bool = True
    model_id: str = "urchade/gliner_multi-v2.1"
    threshold: float = 0.45               # seuil interne de candidature
    labels: dict[str, tuple[str, ...]] = Field(default_factory=lambda: {
        "PERSON": ("personne", "nom de personne"),
        "LOC": ("lieu", "localité", "ville"),
        "ORG": ("organisation",),
    })


class AliasConfig(BaseModel):
    """Règles d'expansion d'alias intra-session (API_DESIGN §5)."""

    titles: tuple[str, ...] = ("M.", "Mme", "Mlle", "Dr", "Pr", "Madame", "Monsieur")
    diminutives: dict[str, str] = Field(default_factory=lambda: {"Momo": "Mamadou"})
    place_shortcuts: dict[str, str] = Field(default_factory=lambda: {"Ouaga": "Ouagadougou"})
    common_names: frozenset[str] = Field(default_factory=lambda: _DEFAULT_COMMON_NAMES)
    pronoun_forms: frozenset[str] = Field(default_factory=lambda: PRONOUN_FORMS)
    max_derived_forms: int = 8            # garde-fou anti-explosion
    enable_initial_forms: bool = False    # R4 (prénom + initiale), off par défaut
    score_cap: float = 0.85               # alias <= 85 % du score canonique
    max_score: float = 0.95               # plafond absolu (seen_count ne dépasse jamais)
    default_canonical_score: float = 0.80


class GaugeConfig(BaseModel):
    """Jauge de quasi-identifiants : densité de catégories, jamais de résolution."""

    window: int = 160
    step: int = 40
    default_threshold: float = 0.50       # appliqué si policy.quasiid_threshold absent
    max_bonus: float = 0.20               # cap du bonus « plus de 4 mentions »


class AfricanConfig(BaseModel):
    """NER ouest-africain : SERENGETI / AfroXLMR / MasakhaNER 2.0 (F-43).

    Chargement lazy, dégradation gracieuse : paquet transformers ou modèle
    absent (hors-ligne) -> détecteur inactif, pipeline continu.
    model_name : serengeti | afroxlmr | masakha (alias MasakhaNER 2.0).
    """

    enabled: bool = True
    model_name: str = "serengeti"
    threshold: float = 0.50               # seuil interne de candidature (la fusion filtre)
    model_ids: dict[str, str] = Field(default_factory=lambda: {
        # SERENGETI : checkpoint de base UBC-NLP (LM) ; configurer un fine-tune
        # NER (ex. MasakhaNER) via model_ids pour l'activer.
        "serengeti": "UBC-NLP/serengeti-E250",
        "afroxlmr": "masakhane/afroxlmr-large-ner-masakhaner-1.0_2.0",
        "masakha": "Davlan/masakhaner2-xlm-roberta-large",
    })


class ThresholdsConfig(BaseModel):
    """Seuils par détecteur (calibration STACK-1 + STACK-6 ; surcharge Policy)."""

    presidio_person: float = 0.45
    presidio_loc: float = 0.40
    spacy: float = 0.50
    gliner: float = 0.45
    serengeti: float = 0.50


class WeightsConfig(BaseModel):
    """Poids d'ensemble par détecteur (vote pondéré de la fusion)."""

    presidio: float = 1.0
    spacy: float = 0.8
    gliner: float = 1.0
    serengeti: float = 1.1
    afro: float = 1.1


# ---------------------------------------------------------------------------
# Configuration racine
# ---------------------------------------------------------------------------


class Config(BaseModel):
    """Configuration globale du sidecar. Priorité : env CLOISON_* > défauts."""

    version: str = "0.1.0"
    grpc_port: int = 50051
    rest_port: int = 8080
    transport: str = "rest"               # "grpc" | "rest" | "both"
    offline: bool = False                 # 1 = aucun téléchargement réseau
    preload: str = "auto"                 # "none" | "auto" (presidio) | "all"
    spacy_size: str = "sm"                # "sm" | "lg" (fr_core_news_*)
    model_cache_gb: float = 6.0
    model_dir: str = "./models"
    budget_seconds: float = 2.0           # deadline douce par requête
    quarantine_seconds: float = 300.0     # pas de rechargement après un crash
    session_mentions_max: int = 200       # borne documentaire (côté core)
    onnx: bool = False
    log_level: str = "INFO"

    presidio: PresidioConfig = Field(default_factory=PresidioConfig)
    gliner: GlinerConfig = Field(default_factory=GlinerConfig)
    african: AfricanConfig = Field(default_factory=AfricanConfig)
    alias: AliasConfig = Field(default_factory=AliasConfig)
    gauge: GaugeConfig = Field(default_factory=GaugeConfig)
    thresholds: ThresholdsConfig = Field(default_factory=ThresholdsConfig)
    weights: WeightsConfig = Field(default_factory=WeightsConfig)

    # -- validation ----------------------------------------------------------
    @field_validator("transport")
    @classmethod
    def _check_transport(cls, v: str) -> str:
        if v not in ("grpc", "rest", "both"):
            raise ValueError(f"transport inconnu: {v!r}")
        return v

    @field_validator("preload")
    @classmethod
    def _check_preload(cls, v: str) -> str:
        if v not in ("none", "auto", "all"):
            raise ValueError(f"preload inconnu: {v!r}")
        return v

    @field_validator("spacy_size")
    @classmethod
    def _check_spacy_size(cls, v: str) -> str:
        if v not in ("sm", "lg"):
            raise ValueError(f"spacy_size inconnu: {v!r}")
        return v

    # -- env (préfixe CLOISON_) ----------------------------------------------
    @classmethod
    def from_env(cls, environ: Mapping[str, str] | None = None) -> "Config":
        """Construit la configuration à partir de l'environnement (CLOISON_*).

        Priorité : env > défauts codés. Les valeurs invalides font échouer la
        validation pydantic (fail-fast au démarrage).
        """
        env: dict[str, str] = dict(os.environ if environ is None else environ)

        def _bool(name: str, current: bool) -> bool:
            raw = env.get(name)
            if raw is None:
                return current
            return raw.strip().lower() in ("1", "true", "yes", "on")

        def _int(name: str, current: int) -> int:
            raw = env.get(name)
            return int(raw) if raw is not None else current

        def _float(name: str, current: float) -> float:
            raw = env.get(name)
            return float(raw) if raw is not None else current

        def _str(name: str, current: str) -> str:
            return env.get(name, current)

        defaults = cls()
        return cls(
            grpc_port=_int("CLOISON_GRPC_PORT", defaults.grpc_port),
            rest_port=_int("CLOISON_REST_PORT", defaults.rest_port),
            transport=_str("CLOISON_TRANSPORT", defaults.transport),
            offline=_bool("CLOISON_OFFLINE", defaults.offline),
            preload=_str("CLOISON_PRELOAD", defaults.preload),
            spacy_size=_str("CLOISON_SPACY_SIZE", defaults.spacy_size),
            model_cache_gb=_float("CLOISON_MODEL_CACHE_GB", defaults.model_cache_gb),
            model_dir=_str("CLOISON_MODEL_DIR", defaults.model_dir),
            budget_seconds=_float("CLOISON_BUDGET_SECONDS", defaults.budget_seconds),
            quarantine_seconds=_float("CLOISON_QUARANTINE_SECONDS", defaults.quarantine_seconds),
            session_mentions_max=_int("CLOISON_SESSION_MENTIONS_MAX", defaults.session_mentions_max),
            onnx=_bool("CLOISON_ONNX", defaults.onnx),
            log_level=_str("CLOISON_LOG_LEVEL", defaults.log_level),
        )
