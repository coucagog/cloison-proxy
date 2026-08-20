# CLOISON — Tests & stratégie

> Chaque couche a une commande exacte, exécutable localement ET en CI
> (`.github/workflows/ci.yml`). Les seuils GO/NO-GO du benchmark (STACK-1 +
> `cloison-detect`) sont rappelés en §6.

## 1. Rust — format, lint, tests

```bash
# Depuis la racine du monorepo fusionné (workspace Cargo.toml) :
cargo fmt --all -- --check              # format
cargo clippy --workspace --all-targets -- -D warnings   # lint (zéro warning)
cargo test --workspace --locked        # unitaires + invariants + e2e (mock)
```

Contenu des suites (par crate) :

| Crate | Tests notables |
|---|---|
| `cloison-core` | invariants bloquants (`tests/invariants.rs`) : roundtrip, aucune valeur claire, anti-collision (sentinelle forgée jamais restaurée), déterminisme, rotation, Luhn ; golden `insta` |
| `cloison-proxy` | `tests/e2e.rs` (mock echo axum in-process) : roundtrip non-stream/stream (sentinelles découpées), sentinelle tronquée → marqueur neutre, tool-calls, 401 sans appel amont, jeton forgé, legacy, models, erreur amont → 502 ; `tests/e2e_audit.rs` (mode audit, reçus) |
| `cloison-audit` | reçus (signature/canonicalité/base64url), k-anonymat (`k-1/k/k+1`), rapport, hash de session « oblivious » |
| `cloison-control` | tenants, émission/rotation/révocation (hash-only), politiques, licences, `validate_token` à temps constant |
| `cloison-ledger` | genèse, append terminal (refus seq/prev_hash/hash/signature), tampering → chaîne cassée, inclusion, ts non décroissant |
| `cloison-verify` | `verify_chain`/`verify_chain_v`/`prove_inclusion`/`find_inclusion`, entrées corrompues |

## 2. Python — sidecar detect (hors-ligne)

```bash
cd services/cloison-detect
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python -m spacy download fr_core_news_sm          # une fois, hors-ligne ensuite
CLOISON_OFFLINE=1 pytest tests -q                 # stubs des gros modèles, aucun réseau
```

`CLOISON_OFFLINE=1` force les stubs (Presidio/GLiNER absents → dégradation
gracieuse) : la suite passe **sans aucun téléchargement ni modèle réel** —
invariant O4 (zéro PII réelle, zéro dépendance réseau en test).

## 3. Benchmark STACK-1 (fossé de détection) — GO/NO-GO

```bash
cd bench/cloison-bench
pip install -r requirements.txt
python run_benchmark.py --seed 42 --output bench/results
```

- Dataset 100 % synthétique (Sénégal : PERSON/LOC/CNI/MAIL/TEL), seed fixe 42 ;
- grille de scoring unique (`scoring.py`) : F1 par type, precision
  adversarial, faux positifs ;
- `run_detect_target.py` (STACK-7) adapte le contrat `POST /detect` au
  format `predictions.jsonl` pour scorer `cloison-detect` sur la **même
  grille** que la baseline Presidio ;
- rapport comparatif + seuils : `journal/STACK-7.md` (§6).

## 4. E2E contre mock (sans réseau)

Les tests `crates/cloison-proxy/tests/e2e*.rs` montent un mock axum
in-process (`CLOISON_UPSTREAM_BASE_URL` → mock) : aucune clé réelle, aucun
réseau externe. Exécution : `cargo test -p cloison-proxy`.

## 5. E2E anti-pass-through + LLM réel (`deploy/e2e_reel.sh`)

```bash
deploy/e2e_reel.sh                    # mode mock (défaut) : aucun secret requis
CLOISON_E2E_MODE=real OPENROUTER_API_KEY=sk-or-v1-... deploy/e2e_reel.sh
```

- **Phase mock (défaut)** — prouve le **masquage amont** (un proxy
  pass-through ÉCHOUE) : un faux LLM local (`deploy/mock_llm.py`, echo +
  journal du corps reçu) est monté dans le réseau docker ; le script ASSERTE
  que le corps reçu par le faux LLM contient des **sentinelles** `⟦…⟧` et
  **pas la PII en clair**, puis que la réponse finale au client contient la
  PII **restaurée** (nom, téléphone `+221`, email) et aucune sentinelle
  résiduelle (regex `⟦[a-z2-7]{26}·[A-Z]{2,4}⟧`) ;
- **Phase réelle** (`CLOISON_E2E_MODE=real|both`) — contre OpenRouter :
  restauration réelle + aucun jeton résiduel (téléphone comparé en chiffres
  normalisés) ;
- le proxy est lancé en `CLOISON_AUDIT_MODE=0` (masquage actif) — le mode
  audit (observe-only) ne masque pas et est couvert par les e2e mock
  (`crates/cloison-proxy/tests/e2e*.rs`).

En CI : job `e2e-llm` (push sur main, `environment: e2e`,
`CLOISON_E2E_MODE=both` : mock puis réel).

## 6. Seuils GO/NO-GO (STACK-7, benchmark seed 42)

| Métrique | Baseline Presidio (STACK-1) | Cible `cloison-detect` | Seuil GO |
|---|---|---|---|
| PERSON F1 | 0.518 | ≥ **0.85** | 0.85 |
| LOC F1 | 0.596 | ≥ **0.85** | 0.85 |
| CNI F1 | 1.000 | ≥ **0.95** | 0.95 |
| MAIL F1 | (baseline) | ≥ **0.95** | 0.95 |
| TEL F1 | (baseline) | ≥ **0.95** | 0.95 |
| Precision adversarial | — | ≥ **0.90** | 0.90 |
| Faux positifs (documents non-PII) | — | ≤ **2 %** | 2 % |

**NO-GO** = un seuil manqué → itérer sur `services/cloison-detect` (seuils,
poids de fusion, gazetteers) et relancer. **GO** = seuils passés + CI verte
+ E2E LLM réel vert → décision de déploiement (wonkom.ai).

## 7. Qualité de la chaîne (CI)

- fmt + clippy (`-D warnings`) + `cargo test --workspace` : obligatoires
  avant tout build d'image (job `images` dépend des jobs rust/python) ;
- pytest detect hors-ligne obligatoire ;
- SBOM (syft) + scans (grype ≥ medium, trivy ≥ HIGH) sur **chaque** image :
  une vulnérabilité HIGH/CRITICAL bloque la CI (invariant O5) ;
- cosign (OIDC) sur main/tags ;
- la doc fait partie du contrôle qualité : `docs/CONFIG.md` est la référence
  des variables, `docs/API.md` des contrats — toute dérive avec le code est
  un bug de CI.

## 8. Commandes rapides (aide-mémoire)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
CLOISON_OFFLINE=1 pytest services/cloison-detect/tests -q
python bench/cloison-bench/run_benchmark.py --seed 42 --output bench/results
deploy/e2e_reel.sh                              # mode mock ; CLOISON_E2E_MODE=real pour le LLM réel
deploy/sbom.sh                                  # SBOM + scans (hors CI)
```
