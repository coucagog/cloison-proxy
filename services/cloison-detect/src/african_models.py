"""Détecteur NER ouest-africain : SERENGETI / AfroXLMR / MasakhaNER 2.0 (F-43).

Chargement LAZY au premier appel (transformers, AutoModelForTokenClassification
+ AutoTokenizer), dégradation gracieuse : paquet transformers absent, modèle
non téléchargeable (hors-ligne) ou prédiction en échec -> `detect()` renvoie
[] et le service continue. Après un crash (OOM, modèle corrompu), le modèle
est mis en quarantaine (même pattern que GlinerDetector).

Voie ONNX (dette ③, journal DEPLOY-8) : quand `CLOISON_ONNX=1` et que le paquet
onnxruntime est présent, l'inférence passe par ONNX Runtime (CPU, quantisation
dynamique int8 par défaut — `CLOISON_ONNX_INT8=0` pour fp32). Le modèle ONNX
est cherché dans `<model_dir>/<model_name>-onnx/` ; s'il manque, il est EXPORTÉ
depuis le modèle transformers (torch.onnx.export, dynamic axes) puis quantisé.
Tout échec ONNX retombe sur le backend torch (jamais d'erreur, jamais de
blocage) — le verdict GO (grille v1.1) est validé sur les deux chemins.

GLiNER n'est PAS concerné (pas d'export ONNX dans gliner 0.2.12 — architecture
span-based, décision documentée DEPLOY-8) : seul le NER africain (afroxlmr,
le goulot de latence) passe par ONNX.

Modèles supportés (config `african.model_name`) :
  - serengeti  : SERENGETI (UBC-NLP) ; le checkpoint de base est un LM, un
                 fine-tune NER (ex. MasakhaNER) doit être configuré via
                 `african.model_ids` pour l'activer ;
  - afroxlmr   : AfroXLMR fine-tuné NER (masakhane, MasakhaNER 1.0+2.0) ;
  - masakha    : alias MasakhaNER 2.0 (Davlan).

Le détecteur ne fait que DÉTECTER ; le filtrage final appartient à la fusion.
"""

from __future__ import annotations

import json
import logging
import os
import threading
import time
from pathlib import Path

from .config import Config
from .spans import Policy, Span, SpanType

logger = logging.getLogger(__name__)

# transformers est OPTIONNEL : s'il est absent, le détecteur se dégrade en
# stub (jamais disponible, detect() -> []), le pipeline continue.
try:  # pragma: no cover - dépend de l'environnement d'exécution
    from transformers import AutoModelForTokenClassification, AutoTokenizer
except ImportError:  # pragma: no cover
    AutoModelForTokenClassification = None  # type: ignore[assignment]
    AutoTokenizer = None  # type: ignore[assignment]

# onnxruntime est OPTIONNEL (voie ONNX, dette ③) : absent -> backend torch.
try:  # pragma: no cover - dépend de l'environnement d'exécution
    import onnxruntime as ort
except ImportError:  # pragma: no cover
    ort = None


# Correspondance labels NER (variantes BIO et plain, fr/en) -> type canonique.
_LABEL_TYPE_MAP: dict[str, SpanType] = {
    "PER": SpanType.PERSON,
    "PERSON": SpanType.PERSON,
    "LOC": SpanType.LOC,
    "LOCATION": SpanType.LOC,
    "GPE": SpanType.LOC,
    "ORG": SpanType.ORG,
    "ORGANIZATION": SpanType.ORG,
    "ORGANISATION": SpanType.ORG,
}

# Noms de modèles acceptés par `african.model_name` (alias masakha inclus).
SUPPORTED_MODELS: tuple[str, ...] = ("serengeti", "afroxlmr", "masakha")


class AfricanModelDetector:
    """NER ouest-africain pluggable (SERENGETI / AfroXLMR / MasakhaNER 2.0).

    Deux backends d'inférence (le backend actif est exposé par `status()` :
    "backend" = "onnx" | "onnx-int8" | "torch") :
      - torch  : chemin historique (transformers, `model(**encoded).logits`) ;
      - onnx   : ONNX Runtime CPU (dette ③) — fp32 ou int8 dynamique selon
                 `config.onnx_int8`, fallback torch à tout échec.
    """

    #: noms de modèles acceptés par `african.model_name`
    supported_models: tuple[str, ...] = SUPPORTED_MODELS

    def __init__(self, config: Config, model_name: str | None = None) -> None:
        self._config = config
        self._model_name = (model_name or config.african.model_name).strip().lower()
        self._lock = threading.Lock()
        self._model = None            # type: ignore[assignment]  # backend torch
        self._session = None          # type: ignore[assignment]  # backend onnx (ort.InferenceSession)
        self._labels_map: dict[int, str] = {}  # id2label (backend onnx)
        self._tokenizer = None        # type: ignore[assignment]
        self._load_attempted = False
        self._quarantine_until = 0.0
        # source pour la fusion (poids d'ensemble) : serengeti | afro
        self.name = "serengeti" if self._model_name == "serengeti" else "afro"

    # -- état ----------------------------------------------------------------
    def available(self) -> bool:
        if time.monotonic() < self._quarantine_until:
            return False
        return self._model is not None or self._session is not None

    def loaded(self) -> bool:
        return self._model is not None or self._session is not None

    def preload(self) -> None:
        self._ensure_loaded()

    def status(self) -> dict[str, object]:
        """État exposé par model_status() (/models, healthz, 503 de l'API)."""
        return {
            "loaded": self.loaded(),
            "available": self.available(),
            "model": self._model_name,
            "model_id": self._model_id() or None,
            "backend": self._backend(),
        }

    def _backend(self) -> str:
        """Backend d'inférence actif : onnx-int8 | onnx | torch | none."""
        if self._session is not None:
            return "onnx-int8" if self._config.onnx_int8 else "onnx"
        if self._model is not None:
            return "torch"
        return "none"

    # -- chargement lazy -----------------------------------------------------
    def _ensure_loaded(self) -> None:
        if self._load_attempted or self.loaded():
            return
        with self._lock:
            if self._load_attempted or self.loaded():
                return
            self._load_attempted = True
            if not self._config.african.enabled:
                logger.info("african: désactivé par configuration")
                return
            if AutoModelForTokenClassification is None or AutoTokenizer is None:
                logger.warning("african: transformers indisponible — détecteur inactif (stub)")
                return
            if self._model_name not in self.supported_models:
                logger.warning("african: modèle inconnu (%r) — détecteur inactif", self._model_name)
                return
            model_id = self._model_id()
            try:
                kwargs: dict[str, object] = {}
                if self._offline():
                    kwargs["local_files_only"] = True  # hors-ligne : cache local uniquement
                tokenizer = AutoTokenizer.from_pretrained(model_id, **kwargs)
                # Voie ONNX (dette ③) : préférée quand activée et disponible.
                if self._config.onnx and ort is not None:
                    if self._try_onnx_backend(model_id, tokenizer, kwargs):
                        logger.info(
                            "african: modèle chargé (%s, backend=%s)",
                            self._model_name, self._backend(),
                        )
                        return
                    logger.warning(
                        "african: backend ONNX indisponible (%s) — repli torch", model_id
                    )
                model = AutoModelForTokenClassification.from_pretrained(model_id, **kwargs)
                model.eval()
                self._tokenizer = tokenizer
                self._model = model
                logger.info("african: modèle chargé (%s, %s)", self._model_name, model_id)
            except Exception as exc:
                logger.warning("african: chargement impossible (%s) — détecteur inactif", exc)
                self._quarantine_until = time.monotonic() + self._config.quarantine_seconds

    # -- backend ONNX (dette ③) ----------------------------------------------
    def _onnx_paths(self) -> tuple[Path, Path, Path]:
        """(chemin du modèle onnx choisi, label_map.json, dossier onnx)."""
        onnx_dir = Path(self._config.model_dir) / f"{self._model_name}-onnx"
        name = "model-int8.onnx" if self._config.onnx_int8 else "model.onnx"
        return onnx_dir / name, onnx_dir / "label_map.json", onnx_dir

    def _try_onnx_backend(self, model_id: str, tokenizer, kwargs: dict[str, object]) -> bool:
        """Charge (ou exporte puis charge) le modèle NER africain en ONNX (CPU).

        Retourne False à tout échec : l'appelant retombe sur torch (jamais
        d'erreur, jamais de blocage). L'export nécessite le paquet `onnx`
        (requirements.txt) ; la quantisation int8 est dynamique (aucune
        calibration — la précision est re-validée par le GO, grille v1.1).
        Layout du dossier `<model>-onnx/` : `model.onnx` (fp32, source
        d'export — avec ses données externes `onnx__*`/`roberta.*` pour les
        gros modèles), `model-int8.onnx` (si int8 demandé, autonome),
        `label_map.json`. Si la quantisation int8 échoue, repli sur le
        fichier fp32.
        """
        model_path, labels_path, onnx_dir = self._onnx_paths()
        fp32_path = onnx_dir / "model.onnx"
        try:
            # 1) s'assurer qu'un fichier fp32 + le label_map existent.
            if not fp32_path.exists():
                onnx_dir.mkdir(parents=True, exist_ok=True)
                import torch  # présent dès que transformers l'est

                model = AutoModelForTokenClassification.from_pretrained(model_id, **kwargs)
                model.eval()
                dummy = tokenizer(
                    "texte d'exemple", return_tensors="pt", truncation=True, max_length=512
                )
                torch.onnx.export(  # type: ignore[union-attr]
                    model,
                    (dummy["input_ids"], dummy["attention_mask"]),
                    str(fp32_path),
                    input_names=["input_ids", "attention_mask"],
                    output_names=["logits"],
                    dynamic_axes={
                        "input_ids": {0: "batch", 1: "seq"},
                        "attention_mask": {0: "batch", 1: "seq"},
                        "logits": {0: "batch", 1: "seq"},
                    },
                    opset_version=17,
                    do_constant_folding=True,
                )
                labels_path.write_text(
                    json.dumps(getattr(model.config, "id2label", {})), encoding="utf-8"
                )
                logger.info("african: modèle ONNX exporté (%s)", fp32_path.name)
            elif not labels_path.exists():
                # ONNX présent sans label_map (provision partiel) : régénérer.
                model = AutoModelForTokenClassification.from_pretrained(model_id, **kwargs)
                labels_path.write_text(
                    json.dumps(getattr(model.config, "id2label", {})), encoding="utf-8"
                )
            # 2) choisir le fichier : int8 demandé ET présent -> int8 ; sinon fp32
            #    (quantisation int8 à la volée si le fichier manque ; échec -> fp32).
            use_path = fp32_path
            if self._config.onnx_int8:
                if model_path.exists():
                    use_path = model_path
                else:
                    try:
                        from onnxruntime.quantization import QuantType, quantize_dynamic

                        quantize_dynamic(
                            str(fp32_path), str(model_path), weight_type=QuantType.QInt8
                        )
                        use_path = model_path
                        logger.info("african: modèle ONNX quantisé int8 (%s)", model_path.name)
                    except Exception as exc:
                        logger.warning(
                            "african: quantisation int8 impossible (%s) — repli fp32", exc
                        )
            # 3) session.
            # NB : PAS de nettoyage des fichiers `onnx__*` / `roberta.*` ici —
            # ce sont les DONNÉES EXTERNES de model.onnx (torch.onnx.export
            # déborde les gros modèles > 2 Go protobuf) : les supprimer casse
            # le chargement fp32 (constat DEPLOY-8). model-int8.onnx
            # (quantize_dynamic) est autonome ; quantize_dynamic nettoie ses
            # propres fichiers temporaires en cas de succès.
            session = ort.InferenceSession(  # type: ignore[union-attr]
                str(use_path), providers=["CPUExecutionProvider"]
            )
            self._labels_map = {
                int(k): str(v)
                for k, v in json.loads(labels_path.read_text(encoding="utf-8")).items()
            }
            self._tokenizer = tokenizer
            self._session = session
            return True
        except Exception as exc:
            logger.warning("african: initialisation ONNX impossible (%s)", exc)
            self._session = None
            return False

    # -- détection ------------------------------------------------------------
    def detect(self, text: str, locale: str = "fr", policy: Policy | None = None) -> list[Span]:
        """Détecte PERSON/LOC/ORG. [] si le modèle est indisponible."""
        if not text:
            return []
        self._ensure_loaded()
        if self._tokenizer is None:
            return []
        if self._session is not None:
            return self._detect_onnx(text, policy)
        if self._model is None:
            return []
        return self._detect_torch(text, policy)

    def _detect_torch(self, text: str, policy: Policy | None) -> list[Span]:
        """Backend torch (transformers) — historique, verdict GO STACK-8/DEPLOY-6."""
        try:
            import torch  # présent dès que transformers l'est

            encoded = self._tokenizer(
                text,
                return_offsets_mapping=True,
                return_tensors="pt",
                truncation=True,
                max_length=512,
            )
            # `offset_mapping` n'est PAS un argument de `forward()` (signature
            # sans **kwargs) : le passer au modèle levait une TypeError — le
            # détecteur africain renvoyait [] EN SILENCE avec transformers
            # 4.46.3 épinglé (correction DEPLOY-6 ; le venv non-pinné du
            # STACK-8 tolérait le kwarg — d'où le verdict GO mesuré).
            offsets = encoded.pop("offset_mapping")
            with torch.no_grad():
                logits = self._model(**encoded).logits
            pred_ids = logits.argmax(dim=-1)[0].tolist()
            probs = torch.softmax(logits, dim=-1)
            spans = self._align_spans(offsets, pred_ids, probs)
        except Exception as exc:
            logger.warning("african: prédiction torch échouée (%s) — spans ignorés", exc)
            self._quarantine_until = time.monotonic() + self._config.quarantine_seconds
            return []
        return self._filter_types(spans, policy)

    def _detect_onnx(self, text: str, policy: Policy | None) -> list[Span]:
        """Backend ONNX Runtime (CPU, int8 dynamique) — dette ③.

        Même contrat que torch (logits → argmax → softmax → alignement) : les
        spans produits doivent être identiques modulo la précision int8 (re-
        validés par le GO, grille v1.1).
        """
        try:
            import numpy as np

            encoded = self._tokenizer(
                text,
                return_offsets_mapping=True,
                return_tensors="np",
                truncation=True,
                max_length=512,
            )
            offsets = encoded.pop("offset_mapping")
            wanted = {i.name for i in self._session.get_inputs()}
            session_inputs = {k: v for k, v in encoded.items() if k in wanted}
            logits = self._session.run(None, session_inputs)[0]  # (1, seq, labels)
            pred_ids = logits.argmax(axis=-1)[0].tolist()
            # softmax stable (numpy) — équivalent torch.softmax.
            e = np.exp(logits - logits.max(axis=-1, keepdims=True))
            probs = e / e.sum(axis=-1, keepdims=True)
            spans = self._align_spans(offsets, pred_ids, probs, labels_map=self._labels_map)
        except Exception as exc:
            logger.warning("african: prédiction ONNX échouée (%s) — spans ignorés", exc)
            self._quarantine_until = time.monotonic() + self._config.quarantine_seconds
            return []
        return self._filter_types(spans, policy)

    def _align_spans(
        self,
        offsets,
        pred_ids: list[int],
        probs,
        labels_map: dict[int, str] | None = None,
    ) -> list[Span]:
        """Aligne tokens -> offsets caractères ; regroupe les tokens contigus
        de même type en spans (gère les préfixes BIO)."""
        if labels_map is None:
            cfg = getattr(self._model, "config", None) if self._model is not None else None
            labels_map = getattr(cfg, "id2label", {}) if cfg is not None else {}
        id2label: dict[int, str] = labels_map or {}
        offsets = offsets[0].tolist()

        spans: list[Span] = []
        current_type: SpanType | None = None
        current_start = 0
        current_end = 0
        token_probs: list[float] = []

        def flush() -> None:
            nonlocal current_type, token_probs
            if current_type is None or not token_probs:
                return
            span = self._make_span(current_type, current_start, current_end, token_probs)
            if span is not None:
                spans.append(span)
            current_type = None
            token_probs = []

        for i, (tok_start, tok_end) in enumerate(offsets):
            if tok_start is None or tok_end is None or tok_start >= tok_end:
                flush()  # token spécial / padding
                continue
            t = self._type_of_label(str(id2label.get(int(pred_ids[i]), "O")))
            if t is None:
                flush()
                continue
            prob = float(probs[0, i, int(pred_ids[i])].item())
            if t == current_type:
                current_end = tok_end
                token_probs.append(prob)
            else:
                flush()
                current_type = t
                current_start = tok_start
                current_end = tok_end
                token_probs = [prob]
        flush()
        return spans

    def _make_span(
        self, stype: SpanType, start: int, end: int, probs: list[float]
    ) -> Span | None:
        """Score = probabilité moyenne des tokens ; seuil interne bas (la fusion filtre)."""
        score = sum(probs) / len(probs) if probs else 0.0
        if score < self._config.african.threshold:
            return None
        return Span(start=start, end=end, type=stype, score=round(score, 4), source=self.name)

    @staticmethod
    def _type_of_label(label: str) -> SpanType | None:
        """Label NER -> SpanType canonique (BIO : B-PER/I-PER, plain : PER...)."""
        raw = str(label).strip()
        if not raw or raw in ("O", "OUT"):
            return None
        core = raw
        if core[:1] in ("B", "I", "E", "S") and "-" in core:
            core = core.split("-", 1)[1]
        return _LABEL_TYPE_MAP.get(core.upper())

    @staticmethod
    def _filter_types(spans: list[Span], policy: Policy | None) -> list[Span]:
        """Filtrage par types demandés (optimisation ; la fusion refiltre)."""
        if policy is None or not policy.types:
            return spans
        return [s for s in spans if s.type in policy.types]

    # -- utilitaires ----------------------------------------------------------
    def _model_id(self) -> str:
        """Identifiant Hugging Face du modèle configuré ('' si inconnu)."""
        return str(self._config.african.model_ids.get(self._model_name, "") or "")

    def _offline(self) -> bool:
        """Hors-ligne : config.offline OU env CLOISON_OFFLINE (aucun téléchargement)."""
        raw = os.environ.get("CLOISON_OFFLINE", "")
        return self._config.offline or raw.strip().lower() in ("1", "true", "yes", "on")
