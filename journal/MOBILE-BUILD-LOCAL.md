?# CLOISON — MOBILE-BUILD-LOCAL : APK Android construit sur cette machine (sans runners GitHub)

> Journal de développement — écrit au fil de l'eau. Gabarit : charte §13.
> Session du 29/08/2026. Demande pilote : « enchaîne avec l'option 2 »
> (construire l'APK en local au lieu d'attendre la reprise des runners GitHub).
> Références : `mobile/android/README.md`, `STACK-N0V13.md` §12, décisions
> pilote 27-28/08 (Android d'abord, WebView + WASM, coffre in-memory).

## Objectif

Produire **`mobile/android/app/build/outputs/apk/debug/app-debug.apk`** sur la
machine Windows locale, sans Android Studio ni runners GitHub, en suivant le
périmètre v1 acté (app WebView + moteur WASM `@cloison/core`, coffre
in-memory, LLM configurable).

## Décisions

1. **Toolchain locale épinglée** dans `_open_design/android-tools/` : JDK 17
   (Temurin zip, sans admin), Android cmdline-tools (sdkmanager), plateforme
   `platforms;android-34` + `build-tools;34.0.0` (compileSdk 34, AGP 8.5.2),
   Gradle 8.9 (bin zip), Rust minimal stable + cible `wasm32-unknown-unknown`
   + `wasm-pack` 0.13.1 (artefact WASM).
2. **Pas d'Android Studio** : tout en CLI (README §Build). APK **debug**
   (installable via `adb install`) ; le release signé exige un keystore
   (hors périmètre de cette session).
3. **Build reproductible** : script `_open_design/build-apk.ps1` (wasm-pack →
   gradle assembleDebug), consigné ici.
4. **Hors sandbox** : téléchargements (egress) + exécutions de gradle/java/
   cargo (spawn + pipes) refusés en mode confiné — jobs arrière-plan élargis.

## Ce qui a été construit

- Toolchain `_open_design/android-tools/` (jdk-17.x, sdk/, gradle-8.9/, cargo/).
- `_open_design/build-apk.ps1` — build reproductible (prérequis vérifiés au
  démarrage, erreurs explicites).
- APK debug (résultat §Résultats).

## Comment lancer / tester (runbook)

```powershell
# Une seule fois (déjà fait) : installer la toolchain (§Toolchain).
# Build complet :
Set-ExecutionPolicy -Scope Process Bypass -Force
& "C:\Users\hp\Desktop\My_Projects\CLOISON_PROJECT\_open_design\build-apk.ps1"

# Installer sur un téléphone (débogage USB activé) :
& "<android-tools>\sdk\platform-tools\adb.exe" install -r `
  "C:\Users\hp\Desktop\My_Projects\CLOISON_PROJECT\cloison\mobile\android\app\build\outputs\apk\debug\app-debug.apk"

# Vérifier sur l'appareil : SettingsActivity → Base URL + clé LLM (votre
# fournisseur), puis chat : le fournisseur ne reçoit que des jetons ⟦…⟧.
```

## Toolchain (état d'installation)

| Composant | Source | État |
|---|---|---|
| JDK 17 | api.adoptium.net (Temurin zip) | ✅ `android-tools\jdk-17.0.20.1+1` |
| cmdline-tools | dl.google.com (`commandlinetools-win-11076708_latest.zip`) | ✅ `android-tools\sdk\cmdline-tools` |
| platform-tools + platforms;android-34 + build-tools;34.0.0 | sdkmanager | ✅ (licences acceptées — adb/android.jar/aapt2 vérifiés) |
| Gradle 8.9 | services.gradle.org (bin zip) | ✅ `android-tools\gradle-8.9` |
| Rust stable 1.98.0 (minimal) + wasm32-unknown-unknown | rustup (static.rust-lang.org) | ✅ `android-tools\cargo` |
| wasm-pack 0.13.1 | GitHub rustwasm (wasm-pack-init.exe) | ✅ `android-tools\cargo\bin\wasm-pack.exe` |

**Leçons d'installation (Windows / PowerShell 7.3+)** : `$PSNativeCommandUseErrorActionPreference=$false` requis avant les installateurs (sinon chaque ligne stderr = erreur fatale avec `ErrorActionPreference=Stop`) ; `wasm-pack-init` exige `rustup` dans le PATH ; `curl -C -` reprend les téléchargements lents (dl.google.com ~100 Ko/s → > 10 min).

## Résultats (session terminée)

- **✅ APK debug PRODUIT (30/08, 01:45)** :
  `cloison\mobile\android\app\build\outputs\apk\debug\app-debug.apk` — **6,1 Mo**,
  `BUILD SUCCESSFUL in 8m38s` (37 tâches), moteur WASM embarqué
  (`cloison_wasm_bg.wasm` 1,79 Mo + glue 21 Ko dans `assets/cloison/pkg`).
- **3 corrections apportées au repo (à committer/pusher — dette §Dettes)** :
  1. `crates/cloison-wasm/Cargo.toml` : ajout `[lib] crate-type =
     ["cdylib","rlib"]` — **wasm-pack échouait** (« crate-type must be
     cdylib ») ; le `cargo check` de la CI ne détecte pas ce manque, et le
     build mobile n'avait JAMAIS été exécuté (runners en panne) → gap réel.
  2. idem : `[package.metadata.wasm-pack.profile.release] wasm-opt = false` —
     le wasm-opt (binaryen) téléchargé par wasm-pack est trop vieux : son
     validateur rejette les opérations bulk-memory (memory.copy/fill) de la
     toolchain actuelle.
  3. Pas de MSVC sur cette machine → les build-scripts Rust (hôte) ne
     pouvaient pas se lier (`link.exe` GNU de Git intercepté → « extra
     operand »). Solution sans admin : **GCC portable winlibs 16.2.0**
     (`android-tools\winlibs\`) + **toolchain Rust GNU** en second toolchain
     (`RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu` posé par
     `build-apk.ps1`). Le wasm final reste produit par wasm-ld (inchangé).
- Warnings à connaître : Kotlin `onBackPressed` déprécié (→
  `OnBackPressedDispatcher`, dette mobile) ; `static_mut_refs` ×6 dans
  `cloison-core/src/wasm.rs` (compat Rust 2024, pré-existant) ; wasm-bindgen
  « license key set but no LICENSE file » (cosmétique).

## Dettes

- **Commit + push** des 2 corrections de `crates/cloison-wasm/Cargo.toml`
  (flux bundle → VPS → GitHub ou à la reprise des runners) ; les rapporter
  aussi au dépôt public `coucagog/cloison-proxy` (le mobile/ y est publié).
- **`mobile-build.yml` (CI)** : vérifier qu'il applique les mêmes corrections
  (il utilisera wasm-pack → cdylib requis) — sinon le job échouera comme ici.
- Test humain sur téléphone réel (`adb install -r app-debug.apk`) — porte
  finale de cette session, côté pilote.
- Dette antérieure inchangée : runners GitHub en panne (CI officielle,
  builds iOS/macOS, release signée).

## Porte de sortie

- [x] APK debug produit par `build-apk.ps1` (build 100 % local, sans runners).
- [ ] `adb install` documenté + vérification humaine sur téléphone (pilote).

## Prochaine étape

Test sur appareil réel (pilote) ; puis reprise des runners → `mobile-build.yml`
(CI officielle) + build iOS réel (macOS requis).

---

## Résultats (complété en fin de session)

**APK debug livré sur cette machine le 30/08/2026** (voir §Résultats
ci-dessus). Le module WASM de tokenisation est embarqué : l'app pseudonymise
in-app avant d'appeler le fournisseur LLM (périmètre v1). Reste le test sur
téléphone réel par le pilote (`adb install`), puis la propagation des
corrections Cargo.toml dans le repo et la CI.
