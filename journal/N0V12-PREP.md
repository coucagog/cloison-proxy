# CLOISON — N0V12-PREP : Préparation de la session N0 v1.2

> Handoff de préparation, écrit en fin de session N0 v1.1 (26/08/2026). À lire
> AVANT la session suivante. Complète `journal/STACK-N0V11.md`
> (§Prochaine étape, §Questions ouvertes), `docs/N0.md` (§7), la charte
> `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§6.1 couches 2-3, §11, §16, règle §5
> de la grille v1.1) et les handoffs `REPRISE.md` / `REPRISE-DEPLOIEMENT.md`.

---

## 1. Pourquoi la session N0 v1.2

N0 v1.1 est **livré** (STACK-N0V11, commits `f9a3bb2a` → `73f84995`) :
① alias intra-session R1–R7 + jauge quasi-id in-core, ② keychain OS pour la
passphrase, ③ module navigateur `@cloison/core` (tokenize/restore
in-browser, démo `deploy/wasm-demo/`), open-core **v0.2.4** publié et vérifié
(core/audit/proxy). CI verte sur main (`73f8499` success, vérifié 26/08).

La **dernière limite assumée de N0** (docs/N0.md §4.1) : le rappel
PERSON/LOC en **texte libre** — un nom hors gazetteer et jamais mentionné
peut partir en clair. Les deux leviers documentés étaient l'alias (livré ①)
et le **NER léger embarqué (ONNX)** — c'est le chantier ④, **le plus lourd**,
qui nécessite un **arbitrage GO/NO-GO en ouverture** (N0V11-PREP §2.4 :
taille du modèle, latence CPU, re-validation GO — règle §5 de la grille si
le benchmark est touché).

## 2. Périmètre de la session (ordre recommandé)

1. **Arbitrage ④ — NER léger embarqué (ONNX) : GO/NO-GO en ouverture.**
   - **Si GO** : export ONNX int8 d'un détecteur PERSON/LOC léger (la voie
     ONNX de `cloison-detect` — DEPLOY-8 — est la référence technique),
     embarquement dans le daemon N0 (runtime ONNX **Rust** — le daemon reste
     « moteur Rust seul », jamais un sidecar Python), mesure latence CPU,
     **re-validation GO grille v1.1** (règle §5 — le benchmark doit rester
     vert, baseline officielle 0.7501 gravée), calibration.
   - **Si NO-GO** : report **documenté** (justification + conditions de
     revue), la limite §4.1 reste assumée.
2. **Open-core v0.2.5** : les chantiers ② (proxy : `keyring`) et ③
   (`cloison-wasm` : ré-export + packaging) changent ces crates →
   re-publication **proxy + wasm** (mécanique validée par v0.2.4 :
   `oc-republish-v4*.sh`, re-split + Cargo.toml autonome + tags + push +
   vérification cargo test des tags). L'audit ne change pas (reste v0.2.4) ;
   le proxy taguera ses deps core/audit **v0.2.4**.
3. **Dettes (si le temps)** : calibration des seuils en prod
   (`measure_clusters.py`, trafic réel), décision IndexedDB chiffré
   navigateur (reportée — clé navigateur sans keychain).

## 3. État de l'existant à réutiliser (ZÉRO réécriture)

| Composant | État (fin N0 v1.1) | Usage v1.2 |
|---|---|---|
| `cloison-core` (Rust) | alias + jauge + gazetteers + `Engine::tokenize_session` | Point d'intégration du NER embarqué (spans PERSON/LOC → tokenisation) |
| Voie ONNX `cloison-detect` (DEPLOY-8) | `export_onnx.py`, ONNX Runtime CPU int8 (afroxlmr), GO re-validé 0.9546 → 0.9560 | **Référence technique** de l'export/embarquement ONNX |
| `cloison-detect` (Python, serveur) | Presidio/GLiNER/afroxlmr — oracle du rappel | Mesure du gain potentiel (fossé vs état N0) |
| `bench/cloison-bench` | grille v1.1 FIGÉE, baseline officielle 0.7501, `run_detect_target.py`, `measure_clusters.py` | Re-validation GO (règle §5) si le benchmark est touché |
| Runtime ONNX Rust | à choisir (candidats : `ort` / `burn` / appels onnxruntime via FFI) | Embarquement sans sidecar Python |
| Open-core | v0.2.4 publié + vérifié ; scripts `oc-*.sh` sur le VPS (mécanique v0.2.4 roliste) | Re-publication v0.2.5 (proxy + wasm) |
| `deploy/wasm-demo/` | page de démo (③) — zéro secret | Référence du packaging navigateur |
| VPS | rustdev rust:1.97 (monte `/src` = repo), modèles HF sur l'hôte (afroxlmr), stack prod saine | Gates + re-publication + (si GO) mesure |

## 4. Décisions techniques à trancher en ouverture

1. **④ GO/NO-GO — critères à figer AVANT le run** (esprit grille
   pré-enregistrée) : modèle candidat (GLiNER-base ou équivalent, taille
   ≤ ? Mo int8), **gain de rappel mesuré** sur le jeu STACK-1 (PERSON/LOC)
   vs l'état N0 actuel (gazetteers + alias), **latence CPU acceptable**
   (cible ? s/doc — la prod actuelle mesure ~0,5 s court / ~1,7 s 160 mots
   via sidecar), impact sur la **spécificité** (ne pas régresser sous 0.60).
   Re-validation **grille v1.1 complète** obligatoire si le benchmark est
   touché (règle §5 — la grille reste FIGÉE).
2. **Runtime ONNX embarqué** : crate Rust (`ort` nécessite la lib
   onnxruntime ; alternatives pures Rust) — à valider pour le build WASM
   (le core compile wasm32 ; le NER embarqué est-il requis en wasm ? → si
   non, gate `cfg(not(target_arch = "wasm32"))`).
3. **Open-core v0.2.5** : périmètre exact (proxy + wasm), tags de deps
   (core/audit → v0.2.4), version du Cargo.toml autonome.
4. **Priorité des dettes** transverses (calibration, IndexedDB) selon le
   temps restant.

## 5. Déroulé proposé

1. **Ouverture** : arbitrage ④ (critères §4.1 figés) + ordre de la session.
2. **④ si GO** : export ONNX léger → embarquement Rust (bindings des spans
   PERSON/LOC dans `Engine::tokenize_session`, dégradation gracieuse si le
   modèle est absent) → mesure latence → **re-validation GO grille v1.1**
   (run benchmark, baseline officielle) → calibration fine.
3. **Open-core v0.2.5** : re-publication proxy + wasm (mécanique v0.2.4) +
   vérification cargo test des tags.
4. **Portes** : cargo test/clippy/fmt (rust:1.97) + e2e N0 + (si ④) WASM ;
   docs (`docs/N0.md` §4.1/§7, CONFIG, README) + journal `STACK-N0V12.md` +
   push + handoffs.

## 6. Prérequis — TOUS SOLDÉS ✅ (vérifiés 26/08/2026)

- CI GitHub **verte sur main** (`73f8499` = chantier ③ : success).
- N0 v1.1 ①②③ livrés ; open-core v0.2.4 publié + vérifié (cargo test des
  tags, rust 1.97) ; e2e_n0 8/8 ; invariants 17 verts.
- Stack prod saine (edge/control/detect/journal/postgres, ONNX int8,
  memwatch 0 OOM — à re-confirmer au boot de session).
- VPS : rustdev (rust:1.97, cible wasm32), outillage `oc-*.sh`, modèles HF
  sur l'hôte (afroxlmr 2,1 Go — référence ONNX DEPLOY-8).
- Décisions pilote : DNS `dsh.wonkom.ai` (retrait **validé** — action
  opérateur anycast.me **toujours en attente**) ; mode audit interne par
  défaut **validé**.

## 7. Sortie attendue

- **Décision ④ actée** (GO/NO-GO, critères pré-figés) ; si GO : NER léger
  embarqué testé + **GO re-validé grille v1.1** + latence mesurée ; si
  NO-GO : report documenté avec conditions de revue.
- **Open-core v0.2.5** (proxy + wasm) publié + vérifié.
- Journal `STACK-N0V12.md` + push ; docs à jour (`docs/N0.md` §4.1 — la
  limite « texte libre » est levée si GO).

## 8. Dettes transverses (à surveiller)

- **GPU (dette ②)** : toujours en attente (décision d'infra pilote ;
  baseline ONNX de DEPLOY-8 comme référence).
- **DNS `dsh.wonkom.ai`** : suppression = action opérateur (zone
  anycast.me) — le record résout encore (vérifié 25/08).
- **Calibration fine des seuils en prod** : procédure documentée
  (`measure_clusters.py`), à exécuter avec du trafic réel.
- **Formats passeport / permis** : à confirmer auprès de sources normatives
  (détection contextuelle conservée — DEPLOY-10).
- **IndexedDB chiffré navigateur** : décision reportée (clé navigateur sans
  keychain = limite ; le module ③ reste volontairement in-memory).
