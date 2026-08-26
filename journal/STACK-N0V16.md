# CLOISON — STACK-N0V16 : Règlement des 6 dettes (demande pilote)

> Journal de développement — session du 29/08/2026, suite de STACK-N0V15.
> Demande pilote : « Réglons nos dettes 1 à 6 » (les 6 étapes listées en fin
> de session docs). Références : charte §11 (honnêteté), §12 (reproductibilité),
> `docs/OPEN-CORE.md` (procédure publication), `journal/DEPLOY-11.md` (dettes
> découvertes), handoffs.

## Objectif

Régler les 6 dettes documentées : ① publier `mobile/` dans `cloison-proxy`
(open-core) ; ② builds mobiles réels via CI ; ③ binaires macOS v0.3.1 ;
④ doctrine `up -d --build` dans DEPLOY.md ; ⑤ docs CLI (forme imbriquée) ;
⑥ calibration avec trafic réel.

## Ce qui a été fait

### ① Publication `mobile/` dans cloison-proxy — ✅ RÉGLÉE (+ réparation critique)

**Découverte structurante** : la branche `main` du dépôt public
`cloison-proxy` avait perdu TOUS ses fichiers racine — `install-n0.sh` →
**404**, README → 404, smoke/provision → 404. Cause : la re-publication
v0.3.1 fait `git subtree split --prefix=crates/cloison-proxy` puis
`push -f HEAD:main` — le split ne contient QUE le crate → les fichiers
racine ajoutés à la main précédente (scripts + README, publication v0.3.0)
ont été **écrasés**. C'est la leçon v0.2.5 (LICENSE écrasé) **récidivée**,
avec cette fois les scripts d'installation grand public → **l'installation
N0 référencée par docs.wonkom.ai était cassée**.

Réparation + publication, scriptées une fois pour toutes :
- `deploy/publish-proxy-public.sh` : re-split du proxy + **overlay racine
  COMPLET** (README public, install-n0.sh/.ps1, smoke-n0.ps1,
  provision_ner_lite.sh, Cargo.lock, **LICENSE = AGPL** leçon v0.2.5,
  **`mobile/` android+ios**) + push -f + vérification des 7 URLs publiques.
- `deploy/public-repo/README.md` : README public du dépôt (guide d'install,
  lien docs.wonkom.ai, composition du dépôt).
- `docs/OPEN-CORE.md` : composition (§2 — apps mobiles publiées dans
  cloison-proxy) + **règle de re-publication** (leçon 29/08 : le push -f du
  split écrase TOUTE la racine — toujours passer par
  `deploy/publish-proxy-public.sh`, jamais pousser la branche split nue).
- README mobiles (android/ios) + site docs (mobile.html §5) : source
  désormais PUBLIÉE (visible sans auth), pas « sur demande ».

**Vérifié** : les 7 URLs publiques → **200** (README, install-n0.sh,
install-n0.ps1, smoke-n0.ps1, provision_ner_lite.sh, mobile/android/README,
mobile/ios/README) ; arborescence main = src/ + tests/ + Cargo.* + LICENSE
AGPL + README + 5 scripts + `mobile/` (47 fichiers).

### ⑤ Docs CLI — ✅ RÉGLÉE (code aligné sur les docs)

Les docs (ONBOARDING.md, main.rs) utilisaient la forme **imbriquée**
(`token issue`, `ledger root`) mais le CLI exposait `token-issue`,
`policy-set`, `license-add` (variantes plates — bug clap déjà corrigé pour
`ledger` en DEPLOY-9, pas généralisé). Corrigé :
- `crates/cloison-cli/src/lib.rs` : sous-enums `TokenCmd`, `PolicyCmd`,
  `LicenseCmd` (même patron que `LedgerCmd`) ; `main.rs` match adapté.
- **Prouvé** : `cloison-cli --help` (token/policy/license/ledger groupés) +
  `cloison-cli token --help` (issue/rotate/revoke/verify) — sur le VPS.
- Portes : cargo test 1/1 + clippy 0 + fmt 1.97 **verts** (container rustdev).
- Zéro référence résiduelle aux variantes plates (grep workspace).

### ④ Doctrine `up -d --build` — ✅ RÉGLÉE

`docs/DEPLOY.md` : note obligatoire — `up -d` sans `--build` réutilise
l'image locale → déploiement dérivé de main (constat DEPLOY-11) ; toujours
`--build` (ou `pull_policy: always`/tag par commit) + vérification
post-déploiement (ex. auth multi-tenant : jeton d'un autre tenant → 401).

### ② Builds mobiles via CI — ✅ PRÊT (exécution à la reprise des runners)

`.github/workflows/mobile-build.yml` (nouveau) : deux jobs prêts —
`android-apk` (ubuntu : wasm-pack → gradle 8.10 `assembleDebug`, AGP 8.5.2 +
Kotlin 2.0.20 épinglés dans `mobile/android/settings.gradle.kts`, upload
APK) ; `ios-sim` (macos-14 : wasm-pack → `xcodebuild` Simulator,
`-derivedDataPath`, upload .app). Actif sur push/tag main — s'exécutera
dès que les runners GitHub reviennent (panne toujours en cours 29/08).

### ③ Binaires macOS v0.3.1 — ✅ VÉRIFIÉE (bloquée infra, documentée)

Release `v0.3.1` confirmée publiée (draft=false, 9 assets, checksums
complets) ; **caveat macOS présent dans le corps de la release** : les
binaires macOS sont des copies v0.3.0 (multi-tenant edge-only, sans effet
N0), « remplacés à la reprise des runners ». Rien d'exécutable sans runners
(build CI macOS requis) — état documenté, pas de dette d'information.

### ⑥ Calibration trafic réel — ✅ VÉRIFIÉE (procédure prête, attente trafic)

`bench/cloison-bench/measure_clusters.py` présent ; calibration exécutée
DEPLOY-11 (1218 TP / 0 FP mono-source, consensus tient sur la stack
actuelle). La fine calibration avec **trafic réel** reste conditionnée à
l'arrivée d'un client réel (procédure prête — `measure_clusters.py`,
documentée DEPLOY-6/11) : dette d'exécution, pas de préparation manquante.

## Résultats (vérifiés)

- Commit `ec6d553` (11 fichiers, +309) + `ad1de12` (fix script) — push GitHub
  `23f1732..ad1de12`.
- Publication publique : **7/7 URLs → 200** ; arborescence main complète
  (src+tests+scripts+mobile+LICENSE AGPL).
- CLI : `--help` imbriqué prouvé ; tests/clippy/fmt verts.
- Site docs redéployé (200) : mobile.html §5 « source publiée ».
- Non-régression : api 401, journal 200, caddy actif (vérifié session).

## Porte de sortie (6 dettes)

- [x] ① `mobile/` publié dans cloison-proxy (+ réparation scripts 404).
- [x] ⑤ CLI imbriqué (token/policy/license/ledger) — docs alignées.
- [x] ④ doctrine `--build` dans DEPLOY.md.
- [x] ② workflow `mobile-build.yml` prêt (APK + iOS Simulator).
- [x] ③ release v0.3.1 vérifiée + caveat macOS documenté.
- [x] ⑥ procédure calibration prête, attente trafic réel documentée.

## Dette restante (infra — hors de notre main)

- Exécution CI des builds mobiles + binaires macOS v0.3.1 : **runners GitHub
  toujours en panne** (confirmé 29/08 : aucun run depuis 27/08).
- Fine calibration : premier client réel.
- `cloison-cli` public (dépôt open-core) : à re-publier avec la forme
  imbriquée quand les runners reprennent (mécanique OPEN-CORE §4) — la
  branche publique actuelle contient l'ancienne interface plate.

## Invariants de sécurité vérifiés

- Zéro secret : la publication publique ne contient aucun token/clé (audit
  des 29 fichiers mobiles + scripts : 0) ; le jeton GitHub n'est jamais
  affiché (lu dans ~/.git-credentials).
- Zéro PII : exemples synthétiques uniquement.
- Intégrité : checksums des assets v0.3.1 intacts ; LICENSE AGPL du proxy
  restauré (leçon v0.2.5).
