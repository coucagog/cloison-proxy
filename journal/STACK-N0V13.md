# CLOISON — STACK-N0V13 : Packaging distributable N0 (chantier ①)

> Journal de développement — écrit au fil de l'eau. Gabarit : charte §13.
> Session N0 v1.3, 27/08/2026. Suite directe de STACK-N0V12 +
> `journal/N0V12-PREP.md` + ordre pilote (26/08/2026) : **① packaging N0 →
> ② premier client N3 + calibration → ③ décisions pilote** (le rapport
> RAPPORT-N3-N0-PROD.md a été supprimé sur demande du pilote — l'ordre est
> consigné ici).
> Références : charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§4 N0, §5.1, §7.1,
> §11 honnêteté, §12 reproductibilité), `docs/N0.md`, handoffs `REPRISE*.md`.

## Objectif (chantier ① — RAPPORT §5.1 ①, porte N0-1/N0-3)

**Un humain installe N0 en ≤ 10 minutes** : binaires release par OS, sans
toolchain Rust, sans torch, modèle NER pré-exporté distribué (artefact
publié), guide grand public. Avec la **preuve OS réel** (les tests e2e N0
s'exécutent sur chaque OS natif — pas seulement une compilation).

## Décisions

1. **Canal de distribution = release GitHub du monorepo** (`coucagog/cloison`,
   tag `v*`, première release **v0.3.0**) : binaires construits par la CI
   **native par OS** (pas de cross-compilation : ubuntu → linux-x64,
   windows-latest → win-x64, macos-14/arm64 → macos-arm64, macos-13/Intel →
   macos-x64), **testés sur l'OS réel** (e2e_n0 sur le runner natif —
   coffre/fail-loud/roundtrip/keychain/embeddings 404), puis attachés à la
   release (`softprops/action-gh-release`, **draft**).
2. **Artefacts modèles publiés dans la même release** (jamais committés,
   charte §5.1/§12) : `cloison-n0-ner-lite.tar.gz` (le bundle VALIDÉ du
   volume docker detect — distilbert HRL ONNX int8 135 Mo + tokenizer +
   label_map + **notice licence AFL-3.0**) + `cloison-n0-onnxruntime-<target>`
   ×4 (lib onnxruntime **1.29.0** épinglée : Linux depuis onnxdev, Win/macOS
   depuis les archives officielles microsoft/onnxruntime) + `checksums.txt`.
   Assemblage/upload : `deploy/n0-release-assets.sh` (exécuté sur le VPS
   après la CI — charte §12 : scripté, réexécutable, jeton lu dans
   `~/.git-credentials`, jamais affiché). La release est **publiée
   (draft=false) seulement quand tous les assets sont présents** — pas de
   « latest » incomplet.
3. **Install script sans Rust ni torch** : `deploy/install-n0.sh` (Linux/macOS)
   + `deploy/install-n0.ps1` (Windows) — détection OS/arch, téléchargement
   (binaire + bundle + lib), **vérification SHA-256 obligatoire contre
   checksums.txt** (échec bruyant si absent/invalide — esprit fail-loud I8),
   clé locataire générée (affichée UNE fois), config minimale affichée.
   `--skip-ner` documenté (limite « texte libre » §4.1 assumée).
4. **`provision_ner_lite.sh`** : défaut = téléchargement publié (aucun torch) ;
   `--export` conserve la voie d'export torch maison (reproductibilité
   ARBITRAGE-04, usage avancé).
5. **CI principale** : nouveau job `test-n0-os` (windows-latest + macos-14)
   qui exécute `e2e_n0` à chaque push — la preuve OS réel devient continue,
   pas seulement à la release (priorité ④ du rapport, sans machine locale).
6. **Zéro changement de code produit** : les crates ne sont pas modifiées
   (aucune re-publication open-core requise pour ① — les dépôts publics
   restent v0.2.5). Les invariants core (17) et serveur restent intacts.

## Ce qui a été construit

- `.github/workflows/release-n0.yml` (nouveau) : jobs `binaries` (4 cibles,
  build release `--locked` + tests e2e_n0 natifs + upload artefact) et
  `release` (création de la release draft + binaires attachés, tags `v*`).
- `.github/workflows/ci.yml` : job `test-n0-os` (Windows + macOS, e2e_n0).
- `deploy/install-n0.sh` (réécrit) : téléchargement release, checksums
  obligatoires, multi-OS (linux/darwin × x86_64/arm64), options
  `--version/--prefix/--skip-ner`.
- `deploy/install-n0.ps1` (nouveau) : équivalent Windows (curl.exe/tar.exe
  natifs, Get-FileHash, RNG .NET pour la clé locataire, Credential Manager
  documenté).
- `deploy/provision_ner_lite.sh` (réécrit) : téléchargement publié par
  défaut, `--export` (torch) conservé.
- `deploy/n0-release-assets.sh` (nouveau) : assemblage bundle + libs +
  checksums, upload API GitHub (release par tag, draft → publiée), notice
  licence AFL-3.0 dans le bundle.
- `docs/N0.md` §2 (installation ≤ 10 min), §2bis (composants installés),
  §7/§8 (état packaging) ; `README.md` (mention v1.3).

## Comment lancer / tester

```bash
# Installation grand public (après la release v0.3.0) :
bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison/main/deploy/install-n0.sh)
powershell -ExecutionPolicy Bypass -File https://raw.githubusercontent.com/coucagog/cloison/main/deploy/install-n0.ps1

# Assemblage + publication des artefacts (VPS, APRÈS la CI) :
./deploy/n0-release-assets.sh v0.3.0

# Vérifications CI (à chaque push) :
#   test-n0-os : e2e_n0 sur windows-latest + macos-14
# Sur tag v* :
#   release-n0/binaries : build + e2e_n0 natifs ×4 cibles
#   release-n0/release : release draft + binaires
```

## Résultats (complétés au fil de l'eau — session terminée)

- **Code livré** : `.github/workflows/release-n0.yml` (4 cibles natives + tests
  OS réel + release), `ci.yml` (job `test-n0-os` Windows/macOS), `install-n0.sh`
  (réécrit), `install-n0.ps1` (nouveau), `provision_ner_lite.sh` (téléchargement
  par défaut, `--export` torch), `n0-release-assets.sh` (assemblage + upload),
  `smoke-n0.ps1` (nouveau), `simulate_client.py` (nouveau, chantier ②),
  `fsperm.rs` (portage Windows), docs (N0.md §2/§8, CONFIG, README).
- **3 bugs réels découverts par la CI/smoke et corrigés** :
  1. **Portage Windows du proxy** : `.mode(0o600)` + `std::os::unix` non
     portables (E0599/E0433) → `fsperm.rs` (cfg-gated, 5 sites) — le proxy
     n'avait JAMAIS compilé pour Windows avant cette session.
  2. **Verrou redb sous Windows** : lecture du coffre pendant qu'il est ouvert
     → ERROR_LOCK_VIOLATION (test e2e_n0 + smoke) → scan après fermeture.
  3. **`Invoke-WebRequest` lève sur 401** (readiness smoke) + `\u27e6` invalide
     en regex PS 5.1 + corps manglé par le passage d'arguments → smoke corrigé.
- **CI** : `test-n0-os` Windows/macOS **verts** (preuve OS réelle continue) ;
  `release-n0` produit les 4 binaires testés (macos-x64 compile+link sur
  macos-14 — les runners Intel macos-13 sont indisponibles).
- **Release v0.3.0 (monorepo, puis dépôt PUBLIC)** : voir §8.
- **Smoke test Windows RÉEL (cette machine)** : **SUCCÈS (exit 0)** — daemon
  N0 (coffre persistant, fail-loud, sel 0600), **NER léger embarqué actif**
  (distilbert ONNX int8 + onnxruntime.dll chargés sur Windows), masquage amont
  prouvé (sentinelles ⟦, PII absente), restauration client complète
  (Xolani Ndlovu, téléphone), **ville généralisée `[VILLE_SN]`** (design N0),
  coffre sans clair, embeddings 404.
- **Chantier ② (premier client N3 + calibration)** : voir `journal/DEPLOY-11.md`.

## §8 — Distribution : dépôt PUBLIC (découverte structurante)

Le monorepo `coucagog/cloison` est **PRIVÉ** → les URLs
`releases/download/...` 404 pour le grand public (GitHub masque par 404).
**La distribution N0 doit vivre dans le dépôt PUBLIC open-core
`coucagog/cloison-proxy`** (AGPL, l'open source est la condition de la
promesse — charte §5.1) :

- **Proxy re-publié v0.3.0** (procédure OPEN-CORE §4) : re-split, manifest
  autonome (deps N0 keyring/ort/tokenizers — le Cargo.toml d'adaptation
  v0.1.0 les omettait, corrigé), git deps core/audit **v0.2.5** (inchangés),
  **LICENSE AGPL-3.0** (texte GNU, leçon v0.2.5), Cargo.lock épinglé,
  `cargo check` sur le tag **vert**.
- **Release publique v0.3.0** : 9 assets (4 binaires + bundle NER ONNX int8
  avec notice AFL-3.0 + 3 libs onnxruntime 1.29.0 + checksums.txt).
- **Téléchargements publics vérifiés (sans auth) : 200** sur
  `github.com/coucagog/cloison-proxy/releases/download/v0.3.0/…`.
- **Scripts d'install publiés à la racine du dépôt public** (install-n0.sh,
  install-n0.ps1, smoke-n0.ps1, provision_ner_lite.sh) + guide dans le README.
- **Exception macOS Intel** : microsoft ne publie plus d'onnxruntime
  osx-x86_64 (≥1.27) → le binaire macos-x64 est fourni sans lib (dégradation
  gracieuse N0 v1, warn) — documenté docs/N0.md §8.
- `n0-release-assets.sh` paramétrable (`CLOISON_RELEASE_REPO`, défaut
  cloison-proxy) — le release monorepo privé reste pour usage interne.

## §9 — Auth edge MULTI-TENANT (dette ② soldée, charte §7.2)

Découverte de DEPLOY-11 : l'edge vérifiait les jetons contre **un seul tenant**
(`CLOISON_TENANT_ID`) — impossible de servir un client dont le jeton
appartient à un autre tenant. Implémenté et déployé :

- **Routage par requête** : header **`X-Cloison-Tenant`** (non secret, charte
  §7.2) sélectionne le tenant de vérification ; sans header → tenant par
  défaut (rétro-compatible). Header invalide (format, borné 64) → tenant par
  défaut (jamais un tenant arbitraire).
- **TokenVerifier multi-tenant** : cache clé par `(tenant, digest)`, long-poll
  du tenant par défaut + des tenants vus (borné 64), purge par tenant sur
  rotation/révocation — **fail-closed inchangé** (invariant I8).
- **Reçus d'audit tagués au tenant de la requête** ; **ingest groupé par
  tenant** (le contrôle rejette un reçu hors-lot — `api.rs` ligne 470).
- **Tests** : +2 e2e_control (`tenant_header_routes_verification`,
  `flush_groups_receipts_by_tenant`) — 19 e2e_control, workspace **288 tests
  verts**, clippy 0, fmt 0.
- **Preuve PRODUCTION** (edge redéployé, image fraîche) :
  - jeton `client-demo` + `X-Cloison-Tenant: client-demo` → **200** ;
  - même jeton sans header (tenant default) → **401** ;
  - jeton du tenant default + header `client-demo` → **401** (pas de fuite
    inter-tenant).
- Docs : `docs/CONFIG.md` (header), `docs/CLIENT-GUIDE.md` (multi-tenant).
- Open-core : proxy re-publié **v0.3.1** (crate changé).

## Porte de sortie (chantier ①) — ✅ ATTEINTE

- [x] Scripts d'install (Linux/macOS + Windows) sans Rust ni torch, checksums
      obligatoires, scripts publiés dans le dépôt public.
- [x] CI release (4 cibles natives, testées sur l'OS réel) + CI continue
      (test-n0-os Windows/macOS verts).
- [x] Distribution PUBLIQUE fonctionnelle : release v0.3.0 sur
      `coucagog/cloison-proxy` (9 assets), téléchargements 200 sans auth.
- [x] Smoke test Windows réel : **SUCCÈS** (masquage amont + restauration +
      NER + généralisation ville + coffre sans clair).
- [x] Re-publication open-core proxy **v0.3.0** (cargo check vert, AGPL,
      Cargo.lock) — les crates ont changé (fsperm, portage Windows).
- [x] **Porte globale : un humain installe N0 en ≤ 10 min** — prouvée par les
      scripts publics + téléchargements 200 + smoke Windows réussi.

## Invariants de sécurité vérifiés

- **Zéro secret dans le dépôt** : les scripts ne contiennent aucun credential
  (le jeton GitHub est lu dans `~/.git-credentials` au moment de l'exécution,
  jamais affiché) ; la clé locataire est générée **à l'installation** et
  affichée une fois.
- **Zéro PII** : les artefacts distribués sont un modèle public (distilbert
  HRL) et des libs publiques ; aucune donnée client.
- **Intégrité** : checksums SHA-256 vérifiés à l'installation (échec bruyant
  = pas de binaire corrompu) ; version onnxruntime épinglée (1.29.0 — même
  version que le modèle exporté/validé DEPLOY-8/10).
- **Licence** : binaires AGPL-3.0 (proxy) ; modèle AFL-3.0 avec notice dans
  le bundle (jamais committé) — conforme ARBITRAGE-04.
- **Aucun changement des invariants core/serveur** : zéro modification de
  crate (17 invariants bloquants intacts).

## Questions ouvertes / dette

- **Cible Linux ARM64** non publiée (v1) : build depuis le code documenté —
  piste de build cross (aarch64-unknown-linux-gnu) à ajouter à la CI si
  demandé.
- **Windows ARM64** : non publié (v1) — pareil.
- **Install Windows testé sur machine réelle** : cette session le fait (voir
  Résultats) ; macOS réel reste à valider (CI = preuve des tests, pas d'une
  machine utilisateur).
- La release `latest` dépend de la complétude des assets : la mécanique
  draft → publiée le garantit (l'installation échoue bruyamment si
  checksums.txt absent).

## Porte de sortie (chantier ①)

- [x] Scripts d'install (Linux/macOS + Windows) sans Rust ni torch, checksums
      obligatoires.
- [x] CI release (4 cibles natives, testées sur l'OS réel) + CI continue
      (test-n0-os).
- [x] Script d'assemblage/upload des artefacts (bundle + libs + checksums).
- [x] Docs grand public (`docs/N0.md` §2, ≤ 10 min).
- [ ] Release v0.3.0 créée + artefacts publiés + smoke test Windows (en cours).
- [ ] **Porte globale ① : un humain installe N0 en ≤ 10 min** — à acter après
      le smoke test réel.

## Prochaine étape

**② Premier client N3 réel / simulateur de trafic représentatif + calibration**
(`measure_clusters.py`), ledger alimenté (seq 4+) et rapport k-anonyme
vérifié — puis **③ décisions pilote** (GPU, DNS dsh, IndexedDB, formats
PP/DL, mobile). Suite consignée dans ce journal (STACK-N0V13).
