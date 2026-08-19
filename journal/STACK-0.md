# CLOISON — STACK-0 : Fondations

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Poser un environnement de développement **reproductible** et un squelette de monorepo
correctement structuré, documenté, versionné et testable en CI, avant tout code produit.
La porte de sortie de cette étape conditionne la suite : sans fondations saines, rien de ce
qui suit (benchmark, moteur, proxy) n'est fiable.

## Périmètre

**Dans :** structure du monorepo, workspace Cargo, docs fondatrices (ARCHITECTURE,
THREAT-MODEL, SECURITY), CI minimale, conventions de code, décision de licence, README,
journal.

**Hors :** tout code de détection, tokenisation, coffre, proxy, contrôle, ledger. Aucune
implémentation produit. Le benchmark (STACK-1) n'est pas commencé ici.

## Décisions

1. **Licence** : Apache-2.0 pour l'ensemble du monorepo dans un premier temps.
   - La note technique recommande d'examiner AGPL pour la passerelle serveur
     (`cloison-proxy`) afin d'empêcher les forks hébergés fermés.
   - Décision différée à la première release publique : le monorepo est privé aujourd'hui,
     et changer de licence avant publication coûte moins cher qu'après. Consigné ici pour
     ne pas l'oublier.
2. **Edition Rust 2021** : raisonnable pour la compatibilité des dépendances WASM et des
   outils ; pas de fonctionnalité 2024 requise par la stack choisie.
3. **Workspace Cargo unique** : un seul `Cargo.toml` racine, membres = les 7 crates.
   Résolution commune des dépendances (`[workspace.dependencies]`) pour des versions
   cohérentes entre crates.
4. **Séparation stricte** : `cloison-core` ne dépend d'aucun framework HTTP (reste pur,
   compilable WASM) ; `cloison-proxy` est le seul crate qui parle HTTP vers le LLM.
5. **Le dépôt ne contient JAMAIS** : PII réelle, secrets, `.env`, coffre, corpus réel.
   `.gitignore` verrouille les `.env`, les bases locales et les coffres.
6. **CI bloquante** : fmt + clippy + test sur chaque push ; la CI échoue sous le seuil.
   Les tests d'invariants de sécurité (voir docs/SECURITY.md) arrivent avec le code
   qu'ils protègent (STACK-2+), mais le squelette CI est posé maintenant.

## Ce qui a été construit

- Structure complète du monorepo (crates/, services/, bench/, proto/, deploy/, docs/, journal/).
- Workspace Cargo avec les 7 crates, chacune avec un `lib.rs` minimal compilable.
- `README.md` (vue d'ensemble + structure + état).
- `docs/ARCHITECTURE.md`, `docs/THREAT-MODEL.md`, `docs/SECURITY.md` (fondations).
- `LICENSE` (Apache-2.0, texte officiel).
- `.gitignore` (secrets, env, artefacts, PII impossible par construction).
- CI GitHub Actions minimale : fmt, clippy, test Rust.

## Comment lancer / tester

```bash
# Cloner (une seule fois)
git clone https://github.com/coucagog/cloison.git
cd cloison

# Compiler le workspace
cargo build --workspace

# Tests
cargo test --workspace

# Qualité
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Résultats

- `cargo build --workspace` : OK (toutes crates compilent).
- `cargo test --workspace` : OK (aucun test encore, crates vides).
- CI : à valider sur le premier push (les workflows se déclenchent alors).

## Invariants de sécurité vérifiés

- Aucun secret, aucune clé, aucun `.env` dans le dépôt (vérifié par `.gitignore` et
  relecture du contenu avant commit).
- Aucune PII dans le dépôt : aucune donnée de référence ni de test n'est incluse.
- Le journal ne contient que des décisions et des métriques, jamais de contenu.

## Questions ouvertes / dette

- Licence AGPL pour la passerelle : à trancher avant la première publication publique.
- `cloison-corpus` (dépôt privé séparé) : structure à définir, hors de ce monorepo.
- Choix du runner CI (ubuntu-latest) et de la toolchain Rust : à confirmer en fonction
  de la cible WASM (wasm32-unknown-unknown) dès STACK-2.
- SBOM / scan d'images : activé dès que les images Docker existent (STACK-3+).

## Porte de sortie

- [x] Squelette de monorepo versionné et poussé sur `main`.
- [x] Docs fondatrices présentes.
- [x] CI minimale configurée.
- [x] Compilation vérifiée par la CI (run 32315042757, success).

## Prochaine étape

**STACK-1 — Benchmark d'abord** : générateur de jeu d'évaluation synthétique sénégalais
(0 PII réelle), baseline Presidio bien configurée (FR + regex CNI + gazetteers), grille de
scoring pré-enregistrée. GO/NO-GO : la détection cible bat-elle Presidio bien réglé sur
PERSON/LOC/CNI ?
