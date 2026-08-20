# CLOISON — proxy de confidentialité PII compatible OpenAI

> **Nom de travail :** CLOISON. **Dépôt :** `coucagog/cloison` (privé). **Serveur de dev :** `wonkom.ai`.
> **Statut :** chantier STACK-0 (fondations). Projet autonome, indépendant de `mania.sn`.

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
| [`journal/`](journal/) | Journal de développement STACK-N |

## Structure

```
crates/
  cloison-core/      # Rust — détection déterministe + tokenisation + coffre (natif + WASM)
  cloison-proxy/     # Rust (Axum) — passerelle compatible OpenAI
  cloison-control/   # Rust (Axum + Postgres) — plan de contrôle aveugle
  cloison-ledger/    # Rust — journal de transparence vérifiable
  cloison-verify/    # Rust — vérificateur public d'attestation
  cloison-cli/       # Rust — outillage admin/ops
  cloison-wasm/      # Rust — wrapper wasm-bindgen de cloison-core
services/
  cloison-detect/    # Python (FastAPI + gRPC) — NER lourd (optionnel)
bench/
  cloison-bench/     # Python — harnais de benchmark + scoring
proto/               # Contrats protobuf
deploy/              # Dockerfiles, compose, Caddy/Traefik, SBOM
docs/                # ARCHITECTURE / THREAT-MODEL / SECURITY / …
journal/             # STACK-0.md, STACK-1.md, …
```

> `cloison-corpus` (gazetteers, corpus ouest-africain, catalogue des non-détections) vit dans un
> dépôt privé séparé, jamais ici.

## État d'avancement

**STACK-0 → 7 livrés** (voir [`journal/`](journal/)) : cœur Rust testé (invariants de
sécurité bloquants), proxy OpenAI-compatible (non-stream + stream + tool-calls), mode
Audit observe-only (reçus signés, k-anonymat), plan de contrôle aveugle + journal de
transparence vérifiable, sidecar NER Python (Presidio + GLiNER + modèles ouest-africains),
packaging Docker distroless / Helm / CI. E2E prouvé : mock 12/12, LLM réel 8/8.

> **⚠️ GO/NO-GO final (grille v1.1) : verdict à confirmer avec les modèles réels.**
> Le run offline du benchmark (`bench/cloison-bench/results/go_nogo_final.json`) est
> **NO-GO** : PERSON +0.095 (seuil +0.12), LOC +0.017 (seuil +0.15), CNI 0.79 (baseline 1.0),
> spécificité 27 % (min 60 %). Décision stratégique (poursuite / réorientation / abandon)
> à trancher par MLS — voir `journal/REPRISE.md`.
