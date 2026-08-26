# CLOISON Mobile — iOS v1 (app WebView + moteur WASM)

> Déclinaison mobile de CLOISON (charte §4 : N0 — le moteur descend chez le
> client). **GO pilote 27/08/2026 : Android d'abord ; iOS livré en source
> (29/08/2026, même périmètre v1).** App WebView embarquant le moteur WASM
> (`@cloison/core`) — pseudonymisation **in-app**, coffre **in-memory**
> (zéro persistance), chat + endpoint LLM configurable.

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
   les préférences de l'app (UserDefaults ; votre clé amont, jamais transmise
   ailleurs).

## Arborescence

```
mobile/ios/
├── Cloison.xcodeproj/            # projet Xcode (ouvrir → Run)
├── Cloison/
│   ├── CloisonApp.swift          # @main
│   ├── ContentView.swift         # WebView + sheet réglages
│   ├── WebViewController.swift   # WKWebView + pont WKScriptMessageHandler
│   ├── LlmClient.swift           # URLSession (POST /chat/completions)
│   ├── AppPrefs.swift            # UserDefaults (baseUrl, apiKey, model)
│   ├── SettingsView.swift        # écran de configuration
│   └── Web/                      # page de chat + glue WASM
│       ├── index.html / app.js / app.css
│       └── pkg/                  # ARTEFACT DE BUILD (wasm-pack) — non commité
└── README.md
```

## Build (app)

Prérequis : **macOS + Xcode** (15+), **Rust cible wasm32** pour l'artefact
WASM (une seule fois par version) :

```bash
# 1. Artefact WASM (à la racine du dépôt CLOISON) :
cd crates/cloison-wasm
wasm-pack build --target web --out-dir ../../mobile/ios/Cloison/Web/pkg
#    (ou copier un pkg construit : cp -r deploy/wasm-demo/pkg …/Web/pkg)
#    Vérification de compilation WASM :
#    cargo check -p cloison-wasm --target wasm32-unknown-unknown

# 2. App (ouvrir mobile/ios/Cloison.xcodeproj dans Xcode → Run, ou CLI) :
cd mobile/ios
xcodebuild -project Cloison.xcodeproj -scheme Cloison \
  -destination 'generic/platform=iOS Simulator' build
```

L'APK/archive de **release signé** nécessite un compte Apple (code signing) ;
le build Simulator est utilisable directement. Install sur appareil :
`xcodebuild -project … -scheme Cloison -destination 'platform=iOS,id=<device>'`.

### Note technique (pont natif)

Contrairement à Android (pont synchrone `JavascriptInterface`), le pont iOS
est **asynchrone** (`WKScriptMessageHandler.postMessage` → réponse via
`window.__cloisonResolve(id, json)`). La page `app.js` adapte la même logique
de chat. Pour le chargement WASM depuis `file://`, le contrôleur active
`allowFileAccessFromFileURLs` + `allowUniversalAccessFromFileURLs`
(requis par le `fetch` de la glue wasm-pack).

## Sécurité (invariants)

- **Zéro PII persistée** : clé locataire aléatoire en mémoire, coffre
  in-memory, rien sur disque (hors votre clé LLM dans les préférences —
  nécessaire pour appeler votre fournisseur).
- **Zéro secret embarqué** : aucun secret dans le code/l'app.
- **Restauration bornée** : registre d'émission de la requête + MAC
  (invariant I3) — une sentinelle forgée n'est jamais restaurée.
- **Honnêteté** (charte §11) : ne protège pas contre un poste compromis
  (même limite que N0) ; quasi-identifiants signalés, jamais résolus.
- **ATS** : le transport vers le fournisseur est HTTPS (ATS strict) ;
  `NSAllowsLocalNetworking` autorise un LLM local (ex. `http://localhost`).

## Limites v1

- **Mode N0-local uniquement** : l'app tokenise in-app vers un fournisseur
  LLM direct (OpenRouter, DeepSeek…). Le mode « clé composite N3 »
  (pseudonymisation par l'edge CLOISON) est une piste v1.1.
- La session est **perdue à la fermeture** de l'app (in-memory assumé) —
  chaque lancement crée une nouvelle clé locataire (non-liabilité).
- Même limites que N0 : rappel PERSON/LOC sans NER embarqué (ici absent —
  mode N0 v1 gazetteers + alias), poste compromis non protégé,
  quasi-identifiants signalés et non résolus.

## Récupérer la source

La source mobile vit dans le monorepo privé `coucagog/cloison`
(`mobile/android/` + `mobile/ios/`), consultable sur demande. La publication
du dossier `mobile/` dans le dépôt public open-core
`github.com/coucagog/cloison-proxy` (AGPL-3.0) est prévue — la promesse
« le code est ouvert » porte pour l'instant sur le moteur et la passerelle.
