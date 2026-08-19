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

- **STACK-0** : fondations (squelette, CI, docs, décisions) — en cours.
- **STACK-1** : benchmark Presidio / GO-NO-GO (conditionne le produit).
