# CLOISON — STACK-7 : Packaging & déploiement

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Rendre CLOISON **déployable par un humain** : images Docker distroless, compose de dev,
Helm, distribution WASM, CI complète, documentation finalisée, et **preuve de bout en bout
contre un LLM réel** — le rodage qui débloque le GO/NO-GO final du STACK-1.

## Périmètre

**Dans :** `deploy/` (Dockerfiles proxy/control/detect, compose, Helm, e2e, SBOM),
`.github/workflows/ci.yml`, `docs/` (8 fichiers finalisés), benchmark GO/NO-GO avec
cloison-detect comme détecteur cible.

**Hors :** le déploiement réel sur wonkom.ai avec TLS (à faire par l'humain avec les docs),
l'installation des modèles africains lourds (GPU requis, documenté).

## Décisions

1. **Images distroless** : `gcr.io/distroless/cc-debian12:nonroot` (uid 65532), multi-stage,
   une image par rôle (edge/control/detect). Rust 1.97 (Cargo.lock exige ≥1.88).
2. **E2E anti-pass-through** : la preuve du masquage se fait en vérifiant que le corps reçu
   par l'amont contient des **sentinelles ⟦** et PAS la PII — un proxy pass-through ÉCHOUE.
   Faux LLM local (mock_llm.py) + phase réelle (OpenRouter).
3. **Compose** : seuls les ports edge sortent (8787) ; control/detect restent internes ;
   2 réseaux (bord + interne strict) — docker ne publie pas de ports sur réseau internal.
4. **Sondes** : tcpSocket pour le proxy (pas de /healthz), httpGet pour control/detect.
5. **Tag CI** : `edge` poussé sur main (reflète la branche), en plus de latest/sha.
6. **Docs = réalité** : générées à partir du code, vérifiées par QA (header 80 octets,
   SHA-256, ports réels).

## Ce qui a été construit

- `deploy/Dockerfile.{proxy,control,detect}` : multi-stage, distroless, non-root.
- `deploy/docker-compose.dev.yml` : edge + control + detect + réseaux + healthchecks.
- `deploy/helm/` : Chart, values, deployment (sondes correctes), service, ingress, secret, pvc.
- `deploy/e2e_reel.sh` + `deploy/mock_llm.py` : e2e mock (12 assertions) + réel (8).
- `deploy/sbom.sh`, `deploy/.env.example`, `deploy/.gitignore`, `.dockerignore`.
- `docs/` : SECURITY, THREAT-MODEL, ARCHITECTURE, DEPLOY, DATA-MODEL, CONFIG, API, TESTING.
- `.github/workflows/ci.yml` : rust, python, bench, docker+SBOM+scan, e2e-llm.
- Binaire `cloison-control` (main.rs) — le crate était une bibliothèque sans binaire.
- `bench/cloison-bench/run_detect_target.py` : GO/NO-GO final avec cloison-detect.

## Résultats

- **E2E mock : 12/12 PASS** — le faux LLM reçoit des sentinelles, jamais la PII ; le client
  reçoit la PII restaurée. Un proxy pass-through échouerait.
- **E2E réel (OpenRouter, gpt-4o-mini) : 8/8 PASS** — nom, téléphone, email restaurés,
  aucun jeton résiduel. **Le produit fonctionne contre un vrai LLM.**
- **Tests** : 202 Rust + 67 Python = 269 verts ; clippy `-D warnings` : 0.
- **Découvertes en exécutant** (le sous-agent a testé réellement, pas seulement lu) :
  - gazetteers jamais activés par défaut → les noms passaient en clair → **corrigé** ;
  - restauration MAC sur valeur brute au lieu de canonique → noms capitalisés devenaient
    [REDACTED] → **corrigé** (verify_body canonicalise) ;
  - canonicalize ne triait pas les tableaux → policy_hash non déterministe → **corrigé** ;
  - rust:1.85 trop vieux pour le Cargo.lock → 1.97 ;
  - docker compose : réseau internal + ports publiés incompatibles → 2 réseaux ;
  - URL amont : doublon `/v1` quand base_url inclut `/api/v1` → **corrigé** + tests.

## Invariants de sécurité vérifiés

1. **Zéro secret dans les fichiers** : .env.example commenté, secrets en `${VAR:?}`.
2. **Distroless non-root** : uid 65532, read-only + tmpfs.
3. **E2E prouve le masquage** : sentinelles amont, PII absente, restauration client.
4. **Docs sans contrevérité** : vérifiées contre le code par QA (80 octets, SHA-256).
5. **Aucune PII en log** : les logs du proxy ne contiennent ni clair ni mapping.

## Questions ouvertes / dette

- Les modèles africains lourds (SERENGETI E250, AfroXLMR) nécessitent GPU : le GO/NO-GO
  final du benchmark se joue en offline (Presidio + GLiNER légers) — à confirmer avec les
  modèles réels en production.
- `trivy-action@master` non pinné dans la CI (QA P2).
- `[profile.release]` (strip/lto) non configuré — à ajouter avant release.
- TLS Caddy : config prête côté serveur de dev (le conteneur caddy tourne déjà).

## Porte de sortie

- [x] Images Docker distroless construites (3/3).
- [x] E2E mock 12/12 + réel 8/8 : produit prouvé de bout en bout.
- [x] Docs finalisées, CI complète, SBOM.
- [x] Binaire control, URL amont corrigée, gazetteers activés.
- [ ] GO/NO-GO final : en cours (benchmark détecteur cible).

## Prochaine étape

Le rapport final de la séquence STACK-0 → STACK-7 à MLS : ce qui a été livré, les preuves,
les décisions prises, et les choix qui restent (déploiement wonkom.ai, modèles lourds,
grille v1.1, GO/NO-GO).
