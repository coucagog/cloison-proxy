# cloison-detect — sidecar de détection NER (STACK-6)

Sidecar **stateless** de détection NER lourd (optionnel) pour fermer le fossé
de rappel PERSON/LOC mesuré en STACK-1 (baseline Presidio : PERSON F1 0.518,
LOC F1 0.596, CNI 1.0). Contrat : `Detect(text, locale, policy) -> spans[]`
via **gRPC** (proto/detect.proto) avec repli **REST/JSON**.

> **Règle absolue** : le sidecar ne fait QUE **DÉTECTER**. Il ne tokenise
> jamais, il ne pseudonymise jamais, il ne résout jamais d'identité, il ne
> persiste rien. Le core Rust reste la source de vérité de la tokenisation et
> des décisions : il consomme les spans (start/end/type/score) et les valide
> contre sa propre tokenisation.

## Architecture

```
Presidio (oracle FR + regex CNI + gazetteers) ─┐
GLiNER zéro-shot (PERSON/LOC/ORG, lazy) ───────┤→ fusion (vote pondéré, IoS,
                                                │   dédupe core_spans)
                                                ├→ alias intra-session (R1–R7)
                                                └→ jauge quasi-id (signal)
```

- `src/spans.py` — types canoniques : `Span`, `SpanType`, `Policy`,
  `SessionContext`, sérialisation JSON, `iou()`, normalisation texte.
- `src/config.py` — configuration pydantic (env `CLOISON_*` > défauts) :
  modèles actifs, seuils par détecteur, poids d'ensemble, règles d'alias,
  fenêtre de jauge.
- `src/presidio_oracle.py` — oracle de référence Presidio (lazy) + regex CNI
  + gazetteers (déterministes, sans réseau).
- `src/gliner_detect.py` — GLiNER zéro-shot (lazy, dégradation gracieuse :
  paquet/modèle absent → `[]` + log).
- `src/alias.py` — `AliasExpander` intra-session : `Marie Dupont` → `Marie`,
  `Mme Dupont`, `M. Dupont`, `Dupont` (R1–R7). Les pronoms ne sont **jamais**
  traités comme des fuites ; session vide = no-op.
- `src/quasi_id.py` — `QuasiIdGauge` : densité age+acte+date+lieu, score 0..1,
  flag > seuil. **Signale, ne résout pas.**
- `src/detect_service.py` — pipeline complet (fusion, dédupe, alias, jauge,
  budget temps) partagé par les deux transports.
- `src/api.py` — FastAPI (`POST /detect`, `GET /healthz`, `GET /version`,
  `GET /models`) + servicer gRPC + convertisseurs proto ↔ interne.
- `src/main.py` — lancement uvicorn (REST) et/ou gRPC.

## Installation

```bash
cd _stage/stack6/services/cloison-detect
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
# Modèle spaCy pour l'oracle Presidio (une fois ; hors-ligne ensuite) :
python -m spacy download fr_core_news_sm
```

GLiNER et les modèles transformeurs sont chargés **lazyment** (premier appel
qui en a besoin). S'ils sont absents ou hors-ligne, le service répond en mode
dégradé (regex CNI + gazetteers + Presidio si disponible) — jamais de crash.

## Génération du code protobuf (transport gRPC)

```bash
mkdir -p src/gen
python -m grpc_tools.protoc -I proto --python_out=src/gen \
    --grpc_python_out=src/gen proto/detect.proto
```

Sans code généré, `src/api.py` désactive le transport gRPC avec un
avertissement ; le REST reste servi.

## Lancement

```bash
python -m src.main                          # REST (uvicorn) sur :8080
CLOISON_TRANSPORT=grpc python -m src.main   # gRPC seul sur :50051 (nominal)
CLOISON_TRANSPORT=both python -m src.main   # les deux
python -m src.main --check                  # précharge (niveau env) puis exit 0
```

Variables d'environnement (préfixe `CLOISON_`) : `GRPC_PORT` (50051),
`REST_PORT` (8080), `TRANSPORT` (`rest`|`grpc`|`both`), `OFFLINE` (0),
`PRELOAD` (`none`|`auto`|`all`), `SPACY_SIZE` (`sm`|`lg`),
`MODEL_CACHE_GB` (6), `MODEL_DIR` (./models), `BUDGET_SECONDS` (2.0),
`QUARANTINE_SECONDS` (300), `QI_WINDOW` (160), `LOG_LEVEL` (INFO).

## API REST

```bash
curl -s localhost:8080/detect -H 'content-type: application/json' -d '{
  "text": "Marie Dupont a 42 ans. Son acte n° 1847 est enregistré à Ouagadougou.",
  "locale": "fr-BF",
  "policy": {"types": ["PERSON","LOC"], "enable_alias_expansion": true,
             "enable_quasiid_gauge": true},
  "session": {"mentions": [{"key": "Marie Dupont", "type": "PERSON",
                            "locale": "fr-BF", "seen_count": 3}]},
  "core_spans": [{"start": 21, "end": 28, "type": "AGE", "score": 1.0}]
}'
```

Réponse : `{"spans": [{"start":0,"end":12,"type":"PERSON","score":0.93}, ...],
"quasi_id": {"score":0.75,"flagged":true,"signals":["age","act","loc"]}}`.
Erreurs : `400 INVALID_ARGUMENT` (offsets invalides, locale mal formée),
`422` (schéma pydantic), `503 FAILED_PRECONDITION` (modèle demandé
indisponible). Le message d'erreur ne contient **jamais** le texte d'entrée.

## Tests

```bash
python -m pytest tests/ -v
```

Les tests remplacent les gros modèles par des **stubs** (monkeypatch des
détecteurs) : aucun téléchargement, aucun réseau. Ils couvrent spans/JSON,
alias (R1–R7, pronoms, score plafonné, dédupe), jauge quasi-id (densité,
fenêtrage, seuil, zéro résolution) et le pipeline complet (fusion, dédupe
core_spans, dégradation GLiNER, budget, contrat REST).

## Invariants de sécurité

1. Le sidecar ne décide jamais : tout est span/score/flag consommé par le core.
2. Pas de tokenisation : offsets caractères relatifs ; le core valide
   l'alignement et peut rejeter tout span.
3. Pas de résolution : la jauge signale une densité, elle ne chaîne jamais
   âge+date+lieu pour produire une identité (limite assumée et testée).
4. Pas de persistance, pas de logs PII : logging = compteurs + avertissements.
5. Stateless : l'état de session vit dans le core, passé dans chaque requête.
6. Hors-ligne réparable : modèles lazy + dégradation gracieuse en continu.
