# CLOISON — Architecture

> Fondation (STACK-0). Vue d'ensemble : topologie, composants, flux.
> Source détaillée : note technique fondatrice (charte v1) + topologie v2.

## Topologie

```
                     ┌───────────────────────── Chez le CLIENT (edge, N0) ──────────────────────────┐
 Interface IA        │                                                                              │
 (Open WebUI /       │   ┌───────────────┐        ┌─────────────────────────┐                       │
  bolt.diy /   ──────┼──▶│ cloison-proxy │──────▶ │ cloison-core (moteur)   │                       │
  agent Hermes)      │   │ (OpenAI-comp) │  clair │  détection + tokenisation│                      │
   base URL + clé    │   └──────┬────────┘        │  + coffre chiffré local  │                      │
   composite         │          │  jetons         └─────────────┬───────────┘                       │
                     │          │                               │ (option) gRPC/HTTP                │
                     │          │                     ┌─────────▼─────────┐                          │
                     │          │                     │ cloison-detect    │ (sidecar Python NER,     │
                     │          │                     │  recall, alias    │  paliers serveur/enclave)│
                     │          │                     └───────────────────┘                          │
                     └──────────┼──────────────────────────────────────────────────────────────────┘
                                │ jetons + clé LLM du client (sortie réseau)
                                ▼
                       ┌──────────────────┐
                       │ Fournisseur LLM  │  ne reçoit QUE des jetons
                       │ OpenAI / Anthropic│
                       └──────────────────┘

        ────────────────────────── Plan de contrôle CLOUD (aveugle) ──────────────────────────
        │  cloison-control : licences, politiques par locataire (0 PII), réception des reçus  │
        │  cloison-ledger  : journal de transparence append-only vérifiable (compteurs signés)│
        │  cloison-verify  : vérificateur public d'attestation                                │
        ───────────────────────────────────────────────────────────────────────────────────
```

## Composants

| Composant | Langage | Rôle | STACK |
|---|---|---|---|
| `cloison-core` | Rust (natif + WASM) | détection déterministe, tokenisation, coffre | STACK-2 |
| `cloison-proxy` | Rust (Axum) | passerelle OpenAI-compatible, aller/retour | STACK-3 |
| `cloison-detect` | Python (FastAPI + gRPC) | NER lourd optionnel (sidecar) | STACK-6 |
| `cloison-control` | Rust (Axum + Postgres) | plan de contrôle aveugle | STACK-5 |
| `cloison-ledger` | Rust | journal de transparence vérifiable | STACK-5 |
| `cloison-verify` | Rust (natif + WASM) | vérificateur public d'attestation | STACK-5 |
| `cloison-cli` | Rust | outillage admin/ops | au fil des STACK |
| `cloison-wasm` | Rust (wasm-bindgen) | wrapper WASM de cloison-core | STACK-2 |
| `cloison-bench` | Python | harnais de benchmark + scoring | STACK-1 |

## Flux principal (aller)

1. L'interface IA envoie une requête OpenAI-compatible (chat/completions, stream ou non)
   avec une **clé composite** `mn_<jeton_acces>.<clé_amont>`.
2. `cloison-proxy` sépare la clé sur le premier `.` : identifie le locataire (jeton
   rotatif, résolu localement ou via control) et retient la clé amont.
3. Le texte passe par `cloison-core` : détection (structuré → gazetteers → NER sidecar),
   tokenisation (HMAC + sel de session), généralisation des faibles cardinalités,
   stockage coffre chiffré local.
4. Le proxy forwarde **uniquement des jetons** + la clé amont vers le LLM.

## Flux principal (retour)

1. Réponse (stream SSE ou complète) : buffer-and-scan, un jeton coupé entre chunks est
   tamponné jusqu'à résolution.
2. Restauration : uniquement les jetons du registre d'émission de la requête en cours,
   somme de contrôle valide. Échec → blocage ou marqueur neutre + compteur.
3. Le client reçoit les vraies valeurs. Rien n'est persisté côté cloud.

## Principes structurants

- **Deux vitesses** : cœur déterministe portable (Rust/WASM) + sidecar lourd Python
  optionnel. Jamais un artefact unique forcé à faire les deux.
- **Le cloud est aveugle par construction** : 0 PII persistée, coffre au bord.
- **Séparation des préoccupations** : `cloison-core` sans framework HTTP (compilable
  WASM), `cloison-proxy` seul crate réseau vers le LLM.
- **Reproductibilité** : tout l'environnement de dev est scripté (`deploy/`) ; le serveur
  de dev n'est pas une source de vérité, le dépôt l'est.
