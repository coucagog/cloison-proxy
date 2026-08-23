# CLOISON — proxy de confidentialité PII compatible OpenAI

> **Nom de travail :** CLOISON. **Dépôt :** `coucagog/cloison` (privé). **Serveur de dev :** VPS 144.217.81.251 (`wonkom.ai`).
> **Statut :** STACK-0 → 8 livrés, **GO/NO-GO final = GO** (grille v1.1, modèles réels), déploiement DEPLOY-1 → 6 actif, **open-core publié** (DEPLOY-7). Projet autonome, indépendant de `mania.sn`.

Un **proxy de confidentialité PII compatible OpenAI** qui s'intercale entre une interface/agent IA
(Open WebUI, bolt.diy, LibreChat, agents type Hermes) et un fournisseur de LLM (OpenAI, Anthropic…).
Il **pseudonymise** les données personnelles avant qu'elles n'atteignent le modèle et **restaure**
les vraies valeurs dans la réponse — sans que le serveur ne conserve ni ne voie de donnée personnelle.
Le moteur descend **chez le client** (edge) ; le cloud n'est qu'un **plan de contrôle aveugle**.

## Documentation

| Document | Rôle |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Topologie, composants, flux |
| [`docs/THREAT-MODEL.md`](docs/THREAT-MODEL.md) | Adversaires × niveaux de cloisonnement |
| [`docs/SECURITY.md`](docs/SECURITY.md) | Invariants de sécurité (non négociables) |
| [`docs/DEPLOY.md`](docs/DEPLOY.md) | Déploiement (compose, Caddy/TLS, wiring contrôle) |
| [`docs/CONFIG.md`](docs/CONFIG.md) | Référence des variables `CLOISON_*` |
| [`docs/API.md`](docs/API.md) | Surfaces API (edge, control, detect, verify) |
| [`docs/OPEN-CORE.md`](docs/OPEN-CORE.md) | Composition open-core, licences, publication |
| [`journal/`](journal/) | Journal de développement STACK-N / DEPLOY-N |

## Structure

```
crates/
  cloison-core/      # Rust — détection déterministe + tokenisation + coffre (natif + WASM)
  cloison-proxy/     # Rust (Axum) — passerelle compatible OpenAI (AGPL-3.0)
  cloison-control/   # Rust (Axum + Postgres) — plan de contrôle aveugle
  cloison-ledger/    # Rust — journal de transparence vérifiable
  cloison-verify/    # Rust — vérificateur public d'attestation (WASM)
  cloison-audit/     # Rust — reçus signés, k-anonymat, rapports
  cloison-cli/       # Rust — outillage admin/ops
  cloison-wasm/      # Rust — wrapper wasm-bindgen de cloison-core
services/
  cloison-detect/    # Python (FastAPI + gRPC) — NER lourd (Presidio + GLiNER + afroxlmr)
bench/
  cloison-bench/     # Python — harnais de benchmark + scoring (méthodologie publique)
proto/               # Contrats protobuf
deploy/              # Dockerfiles, compose, Caddy, provisionnement, SBOM
docs/                # ARCHITECTURE / THREAT-MODEL / SECURITY / …
journal/             # STACK-0.md … STACK-9.md, DEPLOY-1..4.md, REPRISE*.md
```

> `cloison-corpus` (gazetteers détaillés, corpus ouest-africain, catalogue des non-détections) vit
> dans un dépôt privé séparé, jamais ici. Voir [`docs/OPEN-CORE.md`](docs/OPEN-CORE.md).

## État d'avancement

**STACK-0 → 8 livrés + déploiement DEPLOY-1 → 4 actif** (voir [`journal/`](journal/)) :
cœur Rust testé (invariants de sécurité bloquants), proxy OpenAI-compatible (non-stream + stream +
tool-calls), mode Audit observe-only (reçus signés, k-anonymat), plan de contrôle aveugle + journal
de transparence vérifiable, sidecar NER Python (Presidio + GLiNER + **afroxlmr**), packaging Docker
distroless / Helm / CI, wiring edge→detect (B.1), surface publique du journal (DEPLOY-3), journal
alimenté par le vrai pipeline (DEPLOY-4). E2E prouvé : mock 12/12, LLM réel 8/8.

> **🎉 GO/NO-GO final (grille v1.1, pré-enregistrée) : GO** — rejoué avec les modèles réels
> (`afroxlmr` MasakhaNER) sur CPU : PERSON 0.937 · LOC 0.835 · CNI/MAIL/TEL 1.000 · macro 0.954 ·
> spécificité 0.77 — les 5 conditions simultanées sont remplies (baseline Presidio forte :
> PERSON 0.518, LOC 0.596, spécificité 0.42). Le fossé ouest-africain est prouvé
> (`bench/cloison-bench/results/go_nogo_final.json`, journal `STACK-8.md`).

**Production (VPS 144.217.81.251) :** `api.wonkom.ai` (edge, masquage actif) · `journal.wonkom.ai`
(ledger public + vérification WASM) · stack interne control/detect/postgres, 0 OOM (memwatch),
certs auto-renouvelés (Caddy + sonde J-14).

## Licences (open-core — **publié**, DEPLOY-7)

Les composants ouverts sont **publics** sous `github.com/coucagog/cloison-*`
(branche `main`, tag `v0.1.0`) — l'open source est la condition de la promesse
« nous ne lisons pas » (charte §5.1, journal `DEPLOY-7.md`) :

| Composant | Dépôt public | Licence |
|---|---|---|
| Moteur | [cloison-core](https://github.com/coucagog/cloison-core) | Apache-2.0 |
| Passerelle | [cloison-proxy](https://github.com/coucagog/cloison-proxy) | **AGPL-3.0** |
| Journal | [cloison-ledger](https://github.com/coucagog/cloison-ledger) | Apache-2.0 |
| Vérificateur | [cloison-verify](https://github.com/coucagog/cloison-verify) | Apache-2.0 |
| Mode audit | [cloison-audit](https://github.com/coucagog/cloison-audit) | Apache-2.0 |
| Plan de contrôle | [cloison-control](https://github.com/coucagog/cloison-control) | Apache-2.0 |
| Outillage CLI | [cloison-cli](https://github.com/coucagog/cloison-cli) | Apache-2.0 |
| Wrapper WASM | [cloison-wasm](https://github.com/coucagog/cloison-wasm) | Apache-2.0 |
| Sidecar NER | [cloison-detect](https://github.com/coucagog/cloison-detect) | Apache-2.0 |
| Harnais de bench | [cloison-bench](https://github.com/coucagog/cloison-bench) | Apache-2.0 |

- Passerelle serveur (`cloison-proxy`) : **AGPL-3.0** (anti-forks hébergés fermés, charte §5.1).
- `cloison-corpus` : **privé** (jamais publié).

Détails et procédure : [`docs/OPEN-CORE.md`](docs/OPEN-CORE.md).

## Démarrage rapide

```bash
cp deploy/.env.example .env      # secrets locaux, jamais committés
docker compose --profile db --env-file .env \
  -f deploy/docker-compose.dev.yml up -d --build
# clé composite : Bearer mn_<jeton>.<clé_fournisseur>
```
Voir [`docs/DEPLOY.md`](docs/DEPLOY.md) pour le déploiement complet (TLS, wiring contrôle,
provisionnement, tests PostgresStore).
