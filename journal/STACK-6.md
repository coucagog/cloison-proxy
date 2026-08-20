# CLOISON — STACK-6 : cloison-detect (Python NER ouest-africain)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Construire le **sidecar de détection lourde** (Python) qui améliore le rappel PERSON/LOC —
le terrain même du fossé mesuré en STACK-1 (baseline Presidio : PERSON F1 0.518, LOC F1
0.596). Le sidecar ne fait que **détecter** ; le core Rust reste la source de vérité de la
tokenisation.

## Périmètre

**Dans :** `services/cloison-detect/` — contrat `Detect(text, locale, policy) → spans[]`
(gRPC proto + repli REST), pipeline Presidio (oracle) + GLiNER (zero-shot) + modèles
ouest-africains (SERENGETI/AfroXLMR/MasakhaNER), expansion d'alias intra-session, jauge de
quasi-identifiants.

**Hors :** la tokenisation/restauration (jamais — c'est le core), le déploiement (STACK-7).

## Décisions

1. **Détection seule** : le sidecar ne tokenise jamais, ne restaure jamais, ne persiste
   aucune PII. Il retourne des spans ; le core décide.
2. **Modèles** : Presidio chargé au boot (oracle) ; GLiNER + modèles africains **lazy** (au
   premier appel) avec **dégradation gracieuse** (absent/hors-ligne → [] + log, jamais de
   crash). `CLOISON_OFFLINE=1` pour les tests sans téléchargement.
3. **Fusion** : Presidio → GLiNER → africains → fusion pondérée (cluster + résolution) →
   alias → jauge. Budget de temps (deadline) avec `partial` flag.
4. **Alias** : règles R1–R7 (prénom seul, Mme X, formes dérivées), **jamais les pronoms**
   (utilité vs confidentialité), garde ≥ 2 tokens, score plafonné.
5. **Jauge quasi-id** : densité age+acte+date+lieu fenêtrée, `flagged = score > seuil`
   (seuil 1.0 = désactivée de fait), signale sans prétendre résoudre.
6. **Erreurs sans fuite** : exception handler global au format `{"error":{"code",
   "message"}}` ; les erreurs de validation ne renvoient **jamais** le corps de requête brut.

## Ce qui a été construit

- `proto/detect.proto` : DetectService gRPC (DetectRequest/Response, Span).
- `src/` : config (pydantic), spans (dataclass + Policy), presidio_oracle, gliner_detect,
  african_models (SERENGETI/AfroXLMR/MasakhaNER lazy), alias, quasi_id, detect_service
  (pipeline complet), api (FastAPI : /detect, /healthz, /models), main.
- `tests/` : 67 tests (spans, alias, quasi_id, detect_service, african_models, REST, stubs).
- `conftest.py` : CLOISON_OFFLINE garanti.

## Comment lancer / tester

```bash
cd services/cloison-detect
source .venv/bin/activate            # ou bench/cloison-bench/.venv (lourd : torch)
CLOISON_OFFLINE=1 pytest tests -q    # 67 tests, aucun téléchargement
# Service REST : uvicorn src.main:app --port 8080 ; POST /detect
```

## Résultats

- **Tests** : 67/67 verts (57 STACK-6 + 10 modèles africains), sans téléchargement de modèles.
- **Correctifs QA** (GO conditionnel → corrigé) :
  - F-36 test recall_only : mode explicite (balanced bloque 0.35, recall_only passe).
  - F-01 503 prématuré : model_status cohérent (disponible ≠ chargé), 503 seulement pour
    un modèle explicitement demandé et indisponible.
  - F-05 fuite PII dans les erreurs 422 : exception handler global, jamais le corps brut.
  - F-43 modèles africains absents : AfricanModelDetector câblé (serengeti/afroxlmr/masakha).
  - F-27 double comptage jauge : corrigé (score > seuil, pas >=).
  - Offsets unicode : corrigés (les stubs comptaient mal "à" → 22 au lieu de 21).
- **Dégradation vérifiée** : policy.models=["serengeti"] sans modèle → 503 explicite via
  l'API, spans vides en direct, pas de crash.

## Invariants de sécurité vérifiés

1. **Détection seule** : aucun import d'anonymizer, aucune écriture disque de PII, aucune
   tokenisation (vérifié).
2. **Spans correctes** : offsets en points de code, bornes validées, fusion sans
   chevauchement.
3. **Alias sans pronoms** : les pronoms ne sont jamais traités comme des fuites.
4. **Jauge honnête** : score + signaux + flag, jamais de prétendu résolu.
5. **Erreurs sans PII** : le corps de requête ne fuit jamais dans les réponses d'erreur.

## Questions ouvertes / dette

- Les modèles africains réels (SERENGETI E250, AfroXLMR) ne sont pas téléchargés sur le
  serveur de dev (taille, GPU requis) : le benchmark du fossé complet (GO/NO-GO) se jouera
  en STACK-7 avec les modèles réels OU avec un modèle plus léger calibré.
- La parité gRPC↔REST n'est pas testée automatiquement (transport gRPC non exécuté) :
  à couvrir en STACK-7.
- `period` de l'API audit (STACK-4) et le ledger comme source de vérité : à connecter.

## Porte de sortie

- [x] Contrat Detect (gRPC + REST) respecté.
- [x] Pipeline Presidio + GLiNER + modèles africains (lazy, dégradation).
- [x] Alias intra-session + jauge quasi-id.
- [x] 67 tests verts, zéro fuite PII dans les erreurs.
- [ ] Benchmark du fossé avec modèles réels : GO/NO-GO final en STACK-7.

## Prochaine étape

**STACK-7 — Packaging & déploiement** : Docker distroless multi-stage, docker-compose,
Helm, distribution WASM, docs finalisées (SECURITY/THREAT-MODEL/DEPLOY/DATA-MODEL/CONFIG),
SBOM + scan, release. E2E contre un LLM réel (OpenRouter/DeepSeek) — le rodage qui
débloque le GO/NO-GO final du STACK-1.
