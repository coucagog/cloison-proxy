# CLOISON — STACK-N0V15 : Mobile iOS v1 (déclinaison source)

> Journal de développement — écrit au fil de l'eau. Gabarit : charte §13.
> Session du 29/08/2026. Décision pilote (27/08/2026, §11 STACK-N0V13) :
> « mobile = **Android d'abord** (iOS plus tard, date à venir) ». **Demande
> pilote 29/08 : « Version IOS » → GO exécuté en source**, même périmètre v1.
> Références : `journal/STACK-N0V13.md` §12 (mobile Android), charte
> `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§4 N0, §11 honnêteté, §12
> reproductibilité), `mobile/android/` (miroir).

## Objectif

**iOS v1 en source, miroir de l'app Android** : app WebView + moteur WASM
(`@cloison/core`) — pseudonymisation **in-app**, coffre **in-memory**
(zéro persistance, décision pilote 28/08), chat + endpoint LLM configurable.
Aucun SDK iOS disponible dans la session (pas de macOS/Xcode ; runners GitHub
toujours en panne) → **livraison de source + docs de build** (comme Android),
portes de vérification là où c'est possible sans machine Apple.

## Décisions

1. **Miroir strict de l'Android v1** : mêmes assets Web de chat
   (index.html/app.css identiques), même logique JS de tokenize/restore,
   même périmètre (N0-local, coffre in-memory, LLM configurable).
2. **Pont natif asynchrone** (différence technique obligée) : Android utilise
   un pont synchrone (`JavascriptInterface`) ; iOS utilise
   `WKScriptMessageHandler.postMessage` → réponse via
   `window.__cloisonResolve(id, json)`. La logique de chat reste identique.
3. **`GENERATE_INFOPLIST_FILE = YES`** (Xcode 13+) : pas de fichier Info.plist
   séparé — config via build settings (ATS : HTTPS strict +
   `NSAllowsLocalNetworking` pour un LLM local). Bundle id `com.cloison.app`
   (identique Android, cohérence).
4. **Dossier `Web/` en folder reference** dans la phase Resources : le WASM
   binaire + la glue sont copiés **tels quels** dans le bundle (jamais
   retraités) — nécessaire pour `loadFileURL(allowingReadAccessTo:)`.
5. **Zéro changement des crates** : `cloison-wasm` n'est pas modifié (aucune
   re-publication open-core requise). Le `pkg/` est un artefact de build
   (wasm-pack), non commité (`.gitignore`).
6. **Honnêteté documentaire (charte §11)** : constat — `mobile/` n'est PAS
   encore publié dans `cloison-proxy` (vérifié via l'API GitHub, racine =
   src/tests/Cargo.*). Les README mobiles (Android+iOS) et le site public
   (docs.wonkom.ai/mobile.html) le disent désormais explicitement, et la
   publication du dossier `mobile/` dans le dépôt public est consignée comme
   dette (le site public l'affirmait à tort — corrigé).

## Ce qui a été construit

- `mobile/ios/Cloison.xcodeproj/project.pbxproj` — projet Xcode 15+ (target
  « Cloison », iOS 16+, objectVersion 56, 6 sources + Resources Web).
- `mobile/ios/Cloison/CloisonApp.swift` — `@main`.
- `mobile/ios/Cloison/ContentView.swift` — WebView + sheet réglages.
- `mobile/ios/Cloison/WebViewController.swift` — WKWebView
  (allowFileAccessFromFileURLs + allowUniversalAccessFromFileURLs pour le
  WASM), pont `CloisonIOS` (getConfig / openSettings / sendToLlm async),
  erreurs → JSON lisible par le JS (jamais de secret).
- `mobile/ios/Cloison/LlmClient.swift` — URLSession POST
  `{base}/chat/completions` (corps DÉJÀ tokenisé, clé amont en header,
  timeout 60 s).
- `mobile/ios/Cloison/AppPrefs.swift` — UserDefaults (baseUrl/apiKey/model).
- `mobile/ios/Cloison/SettingsView.swift` — écran de configuration SwiftUI.
- `mobile/ios/Cloison/Web/{index.html,app.css}` — copiés de l'Android
  (identiques) ; `app.js` — logique de chat adaptée au pont asynchrone.
- `mobile/ios/README.md` — build (wasm-pack + xcodebuild), invariants,
  limites v1, note technique du pont.
- Docs à jour : `docs/N0.md` (iOS v1 livré en source), README Android
  (iOS livré, pas « plus tard »), `deploy/docs-site/mobile.html` (iOS +
  build Xcode + correction de la publication mobile).

## Comment lancer / tester

```bash
# 1. Artefact WASM (à la racine du dépôt CLOISON) :
cd crates/cloison-wasm
wasm-pack build --target web --out-dir ../../mobile/ios/Cloison/Web/pkg
#    (ou cp -r deploy/wasm-demo/pkg …/Web/pkg) ; vérif :
#    cargo check -p cloison-wasm --target wasm32-unknown-unknown

# 2. App (macOS + Xcode requis) :
cd mobile/ios
xcodebuild -project Cloison.xcodeproj -scheme Cloison \
  -destination 'generic/platform=iOS Simulator' build
#    ou ouvrir Cloison.xcodeproj dans Xcode → Run.
```

## Résultats

- **Livré en source** (29/08/2026) : projet Xcode complet + 6 sources Swift +
  assets Web + README — zéro secret, zéro PII (aucun secret embarqué, aucun
  exemple réel).
- **Portes possibles sans machine Apple** : `cargo check -p cloison-wasm
  --target wasm32-unknown-unknown` (compilation WASM — identique à Android) ;
  revue du pbxproj (structure Xcode 15 standard, folder reference Web) ;
  cohérence assets Web (copiés depuis l'Android, pont adapté).
- **Non vérifiable dans la session** : compilation Xcode réelle (pas de
  macOS/Xcode) et runners GitHub toujours en panne (confirmé 29/08) → le
  build iOS réel + le build APK Android restent en attente de la reprise.
- **Découverte honnêteté** : `mobile/` absent du dépôt public cloison-proxy
  (vérifié API GitHub) — README mobiles + site public corrigés, dette de
  publication consignée.

## Porte de sortie (mobile iOS v1)

- [x] Source iOS v1 complète (projet Xcode + Swift + Web + README).
- [x] Miroir du périmètre Android v1 (tokenize/restore in-app, coffre
      in-memory, chat + LLM configurable, invariants inchangés).
- [x] Docs de build (wasm-pack + Xcode) — même mécanique qu'Android.
- [x] Docs produit à jour (N0.md, README Android, site docs.wonkom.ai).
- [ ] Build iOS réel (Xcode) — **en attente machine macOS / runners GitHub**.
- [ ] Publication `mobile/` dans `cloison-proxy` (dette documentée).

## Invariants de sécurité vérifiés

- **Zéro secret** : aucun secret dans le code (la clé amont est saisie par
  l'utilisateur dans les réglages, stockée UserDefaults, envoyée uniquement
  au fournisseur configuré).
- **Zéro PII** : aucun exemple réel ; le clair ne quitte jamais l'app
  (tokenisé avant l'appel natif).
- **Restauration bornée** (I3) : registre de la requête + MAC — une
  sentinelle forgée n'est jamais restaurée (via `cloisonRestore`).
- **Honnêteté** (§11) : limites documentées (poste compromis, quasi-ids
  signalés non résolus, session perdue à la fermeture) ; affirmation de
  publication publique corrigée (mobile/ non publié).

## Questions ouvertes / dette

- **Publication `mobile/` dans `cloison-proxy`** : à faire quand les runners
  reprennent (ou décision pilote) — la source reste dans le monorepo privé
  (consultable sur demande) en attendant.
- **Build iOS réel** : nécessite macOS/Xcode (CI macOS-14 à la reprise des
  runners) — procédure prête (`mobile/ios/README.md`).
- Même suite que STACK-N0V14 : APK Android, binaires macOS v0.3.1, docs en
  anglais éventuelle.

## Prochaine étape

À la reprise des runners GitHub : **builds mobiles réels** (APK Android +
app iOS via CI — workflow `mobile-build.yml` PRÊT) puis **binaires macOS
v0.3.1**. Décision pilote : publication de `mobile/` dans l'open-core —
**✅ EXÉCUTÉE (29/08, dette ① réglée)** : `deploy/publish-proxy-public.sh`
a publié `mobile/` (android+ios) + README + scripts d'install dans
`cloison-proxy` (voir STACK-N0V16).

## STACK-N0V16 — Règlement des 6 dettes (29/08/2026)

> Suite de cette session — voir le journal dédié `STACK-N0V16.md` pour le
> détail. Résumé : ① publication mobile open-core RÉPARÉE (scripts d'install
> et README étaient 404 sur main — écrasés par le push -f v0.3.1, leçon
> v0.2.5 récidivée) ; ⑤ CLI imbriqué (token issue/rotate/revoke/verify,
> policy set, license add) — prouvé ; ④ doctrine `up -d --build` dans
> DEPLOY.md ; ② workflow `mobile-build.yml` prêt (APK + iOS Simulator) ;
> ③ release v0.3.1 vérifiée (caveat macOS) ; ⑥ calibration prête (attente
> trafic réel).
