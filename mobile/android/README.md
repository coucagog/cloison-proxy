# CLOISON Mobile — Android v1 (app WebView + moteur WASM)

> Déclinaison mobile de CLOISON (charte §4 : N0 — le moteur descend chez le
> client). **GO pilote 27/08/2026 : Android d'abord ; iOS v1 livré en source
> (29/08/2026, `mobile/ios/` — même périmètre).**
> Périmètre v1 acté (28/08) : **app WebView embarquant le moteur WASM**
> (`@cloison/core`) — pseudonymisation **in-app**, coffre **in-memory**
> (zéro persistance, décision pilote 28/08), chat + endpoint LLM configurable.

## Ce que fait l'app (v1)

1. **Pseudonymisation in-app** : votre message est tokenisé **dans l'app**
   (WASM, clé locataire aléatoire générée côté client) avant d'être envoyé au
   fournisseur LLM — le fournisseur ne reçoit **que des jetons** `⟦…⟧`
   (jamais la PII en clair).
2. **Restauration in-app** : la réponse du LLM est restaurée dans l'app
   (jetons émis par la requête en cours uniquement — registre + MAC).
3. **Coffre in-memory** : rien n'est persisté (choix pilote) ; la session
   WASM (clé locataire + registre) vit dans la mémoire de l'app et meurt à la
   fermeture.
4. **Configuration** : endpoint LLM (Base URL + clé) + modèle — stockés dans
   les préférences de l'app (votre clé amont, jamais transmise ailleurs).

## Arborescence

```
mobile/android/
├── app/src/main/
│   ├── assets/cloison/          # page de chat + glue WASM (index.html, app.js, app.css)
│   │   └── pkg/                 # ARTEFACT DE BUILD (wasm-pack) — non commité, voir §Build
│   ├── java/com/cloison/app/    # MainActivity (WebView+bridge), LlmClient (HTTP), SettingsActivity
│   ├── res/                     # layout, strings, thème
│   └── AndroidManifest.xml
├── build.gradle.kts / settings.gradle.kts / gradle.properties
└── README.md
```

## Build (APK)

Prérequis : **JDK 17**, **Android SDK** (Android Studio recommandé), **Rust
cible wasm32** pour l'artefact WASM (une seule fois) :

```bash
# 1. Artefact WASM (à la racine du repo CLOISON) :
cd crates/cloison-wasm
wasm-pack build --target web --out-dir ../../mobile/android/app/src/main/assets/cloison/pkg
#    (ou copier un pkg construit : cp -r deploy/wasm-demo/pkg …/assets/cloison/pkg)
#    Vérification de compilation WASM :
#    cargo check -p cloison-wasm --target wasm32-unknown-unknown

# 2. APK (ouvrir mobile/android dans Android Studio → Run, ou CLI) :
cd mobile/android
gradle assembleDebug        # → app/build/outputs/apk/debug/app-debug.apk
```

L'APK de **release signé** nécessite un keystore (documenté §Release) ; l'APK
debug est installable directement (`adb install`).

## Sécurité (invariants)

- **Zéro PII persistée** : clé locataire aléatoire en mémoire, coffre
  in-memory, rien sur disque (hors votre clé LLM dans les préférences —
  nécessaire pour appeler votre fournisseur).
- **Zéro secret embarqué** : aucun secret dans le code/l'APK.
- **Restauration bornée** : registre d'émission de la requête + MAC
  (invariant I3) — une sentinelle forgée n'est jamais restaurée.
- **Honnêteté** (charte §11) : ne protège pas contre un poste compromis
  (même limite que N0) ; quasi-identifiants signalés, jamais résolus.

## Limites v1

- **Mode N0-local uniquement** : l'app tokenise in-app vers un fournisseur
  LLM direct (OpenRouter, DeepSeek…). Le mode « clé composite N3 »
  (pseudonymisation par l'edge CLOISON) est une piste v1.1.
- **iOS** : v1 livrée en source (29/08/2026) — `mobile/ios/` (SwiftUI +
  WKWebView + moteur WASM, même périmètre).
- La session est **perdue à la fermeture** de l'app (in-memory assumé) —
  chaque lancement crée une nouvelle clé locataire (non-liabilité, charte §8).
