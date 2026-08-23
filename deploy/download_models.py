"""Téléchargement des modèles NER dans le volume /models (HF_HOME).

Utilisé au premier provisionnement : les modèles doivent être présents dans le
volume AVANT le démarrage du sidecar — le réseau interne `cloison-internal` n'a
aucun egress (THREAT-MODEL §3.1), les téléchargements au boot échouent.

Exécution (sur l'hôte, réseau avec egress, une seule fois par volume) :
    python deploy/download_models.py          # ou depuis deploy/
    # puis : docker compose up -d detect      # HF_HUB_OFFLINE=1 en prod
"""
import argparse

import huggingface_hub as h

# GLiNER multi-lingue (zero-shot PERSON/LOC/ORG) + son backbone.
GLINER = "urchade/gliner_multi-v2.1"
GLINER_BACKBONE = "microsoft/mdeberta-v3-base"  # backbone du GLiNER 0.2.12

# NER ouest-africain (le fossé GO, grille v1.1) : MasakhaNER via AfroXLMR.
AFROXLMR = "masakhane/afroxlmr-large-ner-masakhaner-1.0_2.0"

# spaCy (oracle Presidio) — modèles FR + EN.
SPACY_MODELS = ["fr_core_news_md", "en_core_web_md"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--skip-spacy",
        action="store_true",
        help="ne pas télécharger les modèles spaCy (déjà présents)",
    )
    args = parser.parse_args()

    print("téléchargement GLiNER:", h.snapshot_download(GLINER))
    print("téléchargement backbone GLiNER:", h.snapshot_download(GLINER_BACKBONE))
    print("téléchargement afroxlmr:", h.snapshot_download(AFROXLMR))
    if not args.skip_spacy:
        import spacy  # noqa: PLC0415  (import tardif : le venv du provisionnement)

        for name in SPACY_MODELS:
            print("téléchargement spaCy:", name)
            spacy.cli.download(name)
    print("OK")


if __name__ == "__main__":
    main()
