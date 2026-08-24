# CLOISON — DEPLOY-9 / N3 : Couche commerciale + dette 71/75 + couverture PII étendue

> Journal de campagne — exécution de `journal/REPRISE-DEPLOIEMENT.md` §6
> (décision pilote 23/08/2026 : finir **N3**, régler **71/75** au passage,
> GPU/N0 en sessions ultérieures). Session du 23-24 août 2026.
> Ordre : **① dette 71/75 → ② N3 (couche commerciale) → ③ N3+ (couverture
> PII sénégalaise étendue : fixes, passeport, permis, matricules)**.

## Objectif

1. **① 71/75** : couvrir les nouveaux préfixes mobiles sénégalais **71** et
   **75** (un numéro non détecté part en clair vers le LLM — invariant I1),
   re-valider le GO (grille v1.1, règle §5), redéployer, re-publier
   l'open-core.
2. **② N3** : rendre le produit **achetable** — `cloison-cli` (ops),
   onboarding locataire bout en bout, docs client, rapport de conformité,
   design system des pages.
3. **③ N3+** (demande pilote en cours de session) : couvrir aussi les
   **téléphones fixes (30/32/33/36)**, les **numéros de passeport**, les
   **permis de conduire** et les **matricules des fonctionnaires de l'État
   et de l'IPRES** (actifs et retraités).

## ① — DETTE 71/75 (correctif de couverture)

### Constat & inventaire (§6ter)

Préfixes **71** et **75** existent désormais dans la numérotation mobile
sénégalaise. Inventaire exact (`4ed0c45` → corrigé) :

| Fichier | Avant | Après |
|---|---|---|
| `crates/cloison-core/src/detection.rs` (l.200) | format local `(?:70\|75\|76\|77\|78)` — **71 manquant** (international `+221 7[0-9]` OK) | `(?:70\|71\|75\|76\|77\|78)` |
| `bench/cloison-bench/presidio_baseline.py` | `(?:70\|75\|76\|77\|78)` ×3 + patterns espacés `[67][078]` (71/75/76 absents) | `(?:70\|71\|75\|76\|77\|78)` partout (5 regex + docstring) |
| `bench/cloison-bench/generator.py` | `PREFIXES_TEL = {Orange:[77,78], Free:[76], Expresso:[70]}` | `+ Free:75, Expresso:71` — attribution opérateur à confirmer (ARTP), commenté |
| `bench/cloison-bench/README.md` | table TEL « préfixes 70/76/77/78 » | « 70/71/75/76/77/78 » |
| `bench/cloison-bench/test_benchmark.py` | `startswith(('70','76','77','78'))` | 6 préfixes, 50 tirages |

Attribution opérateur : aucune source publique définitive trouvée (recherche
web 24/08/2026) → répartition **hypothèse de travail** (Free 75, Expresso 71),
documentée dans `generator.py` ; seule la présence des préfixes compte pour
la couverture. À confirmer ARTP (dette documentaire).

### Tests

- Core : nouveau test `test_detect_phone_sn_prefixes_71_75` (8 variantes :
  71/75 × local/concat/international) — **vert**.
- Bench : `test_generate_tel` élargi — **32/32 verts**.
- Detect : non-régression **77/77 verts** (inchangé — pas de logique TEL
  propre dans le sidecar, l'oracle Presidio couvre `PHONE_NUMBER`).

### Re-validation GO (règle §5 — modèles réels, grille v1.1)

Environnement `onnxdev` (VPS, volume `/models`, bench+detect+ONNX). Le jeu
est **régénéré** (seed 42, 500 docs) : les 6 préfixes sont présents dans le
gold (70:74 · **71:81** · **75:58** · 76:74 · 77:75 · 78:62).

| Métrique | seuil | torch | onnx-int8 | verdict |
|---|---|---|---|---|
| PERSON | ≥ 0.638 | **0.9392** | **0.9413** | ✅ / ✅ |
| LOC | ≥ 0.746 | **0.8360** | **0.8366** | ✅ / ✅ |
| CNI | non-régression | 1.0000 | 1.0000 | ✅ / ✅ |
| MAIL / TEL | — | 1.0000 / 1.0000 | 1.0000 / 1.0000 | ✅ / ✅ |
| macro | ≥ 0.850 | **0.9550** | **0.9556** | ✅ / ✅ |
| spécificité | ≥ 0.60 | 0.77 | 0.77 | ✅ / ✅ |

**VERDICT : GO sur les deux chemins** (5/5 PASS chacun). Méthodologie :
la régénération du rapport dérivait la baseline (deps bench non-épinglées,
macro 0.7681 — dette secondaire connue) ; conformément à la doctrine
DEPLOY-6, la **baseline OFFICIELLE gravée (0.7501) a été restaurée** dans
`rapport.json` et le GO ré-évalué contre elle — il tient a fortiori (les
marges contre la baseline dérivée, plus stricte, étaient déjà toutes POSITIVES).
Artefacts : `results/go_nogo_final.tel7175-{torch,onnx}.json` (vs baseline
dérivée) + `go_nogo_final.tel7175-officiel-{torch,onnx}.json` (vs 0.7501).

### E2E contre l'edge DÉPLOYÉ (preuve 71/75)

Harnais `deploy/e2e_reel.sh` rendu paramétrable
(`CLOISON_E2E_PII_*`) et rejoué avec `PII_PHONE="71 123 45 67"` :

- **SUCCÈS** — le corps reçu par le faux LLM contient des sentinelles ⟦ et
  **pas** « 71 123 45 67 » (masquage amont prouvé) ; la réponse client
  contient « 71 123 45 67 » restauré, zéro jeton résiduel.
- Un proxy pass-through échouerait ce test (la PII serait en clair amont).

### Redéploiement

- Image edge rebuildée (le core est embarqué dans le binaire proxy) +
  conteneur recréé — **sans rupture client** (auth par hash du contrôle
  inchangée, mêmes clés composites).
- Stack relancée en config prod : edge/control/detect (healthy, afroxlmr
  chargé)/journal/postgres, 401 sans auth, ledger public 3 lignes.
- Page journal **restylée** (design system de référence, voir ②) : image
  journal rebuildée, `journal.wonkom.ai` → 200.

### Re-publication open-core v0.2.0

- `coucagog/cloison-core` + `coucagog/cloison-bench` : re-split (subtree),
  overlay LICENSE, push `main` + tag **v0.2.0** (procédure
  `docs/OPEN-CORE.md` §4, même mécanique que DEPLOY-8).
- Vérification post-publication : pytest bench sur le tag (32, avec 71/75) +
  cargo test core sur le tag (git deps réelles).

## ② — N3 : COUCHE COMMERCIALE

### `cloison-cli` (rempli — squelettes DEPLOY-7 → outillage ops complet)

Crate `crates/cloison-cli` (clap + reqwest, mêmes versions que le proxy) :

| Commande | Route contrôle | Rôle |
|---|---|---|
| `provision <tenant> --nom --plan [--issue-token]` | POST /admin/tenants + /tokens | onboarding : tenant + licence + jeton |
| `token issue <tenant>` | POST /tokens | émission `mn_` (clair affiché UNE fois) |
| `token rotate <tenant> <id>` | POST /rotate | rotation avec grâce |
| `token revoke <tenant> <id>` | DELETE /tokens/{id} | révocation immédiate |
| `token verify <tenant> <token>` | POST /v1/control/verify | **hash local** — le clair ne quitte jamais le CLI (I2) |
| `policy set <tenant> <json\|->` | PUT /policy | politique par locataire |
| `license add <tenant> --plan [--expires]` | POST /licenses | licences |
| `ledger root` | GET /v1/control/root | racine du journal |
| `ledger check <jsonl> [--pubkey-file]` | — (hors-ligne) | **vérification décentralisée** via cloison-verify (chaîne + signatures) |
| `stats <tenant>` | version + root | vue rapide |

Sécurité : zéro secret en log, zéro PII, `CLOISON_CONTROL_URL` (défaut
127.0.0.1:8788), hash `cloison-mn-token-v1:` identique au contrôle et au
proxy (testé). Compilation/test/clippy/fmt **verts** (rust 1.97, VPS).

**Rodage réel (preuve onboarding contre le contrôle DÉPLOYÉ)** — la preuve a
révélé un bug clap : `ledger` était dérivé en `ledger-root`/`ledger-check`
(variantes plates). Corrigé : sous-commande imbriquée
`#[command(subcommand)] Ledger(LedgerCmd)` → `cloison-cli ledger root|check`.
Re-prouvé : provision (tenant créé + jeton `mn_` émis UNE fois) → `ledger
root` (`{root_hash, seq:2}`) → `stats` — **flux N3 bout en bout validé**.

### Onboarding locataire (documenté + scripté)

- `docs/ONBOARDING.md` : flux complet (provision → émission → vérification
  par hash → livraison de la clé composite), commandes CLI équivalentes,
  vérifications post-onboarding, invariants.
- `deploy/onboard_client.sh` : script bout en bout (tenant → jeton `mn_` →
  verify → clé composite `mn_<jeton>.<clé_amont>` affichée) — le clair est
  lu en mémoire, jamais persisté ni loggé.

### Docs client (la promesse vérifiable)

- `docs/CLIENT-GUIDE.md` : brancher son interface IA (2 champs : Base URL
  `https://api.wonkom.ai/v1` + clé composite), ce qui se passe (edge →
  jetons → restauration), FAQ confidentialité (nous ne voyons rien : journal
  public + open-core + rapport), limites honnêtes (quasi-identifiants, poste
  compromis, PII hallucinée), niveaux N0–N3, rapport de conformité §6.

### Rapport de conformité (présentable au client)

`GET /v1/audit/report?period=hourly|daily|weekly|all` (mode audit
observe-only, `CLOISON_AUDIT_MODE=1`) : compteurs k-anonymes par type,
restaurations incomplètes, sorties bloquées, jauges — **signés Ed25519**
et présentables (aucun texte, aucune cellule < k). Documenté dans
`docs/CLIENT-GUIDE.md` §6 et `docs/API.md` §1.5.

### Design system des pages (référence `Doc_REF/cloison-topologie_PII_V3.html`)

La page du journal public est restylée avec le **design system de référence**
(extrait du HTML `Doc_REF/cloison-topologie_PII_V3.html`) :

- variables CSS (ink/paper/panel, sémantiques clair/jeton/edge/danger avec
  bg+line), **thème sombre** auto-OS + bascule manuelle sans stockage,
  typographies sans/mono, `--wrap:1060px` ;
- header sticky blur + brand, hero dot-grid, bandes `.band`, `.idx`,
  cartes `.card` (ok/ko), notes `.note`, pills, focus-visible,
  `prefers-reduced-motion`.
- Logique WASM (`cloison-verify`) **inchangée** : vérification de chaîne +
  inclusion dans le navigateur. Déployé (`journal.wonkom.ai` → 200,
  `theme-btn`/« Le journal qui prouve » présents).

### Décisions en attente (documentées pour le pilote)

1. **`dsh.wonkom.ai`** : DNS → VPS mais **aucun bloc Caddy ne le sert**
   (ancien hôte du harness dsh). Recommandation : **retirer l'enregistrement
   DNS** (le harness dsh ne vit plus ici) ou le documenter comme dormant.
   En attente de validation pilote (action opérateur pour le DNS).
2. **Mode audit public vs interne** : choix actuel = **interne par défaut**
   (`CLOISON_AUDIT_MODE=0` en prod, masquage actif) ; le rapport de
   conformité est **disponible par tenant client** en mode observe-only
   (opt-in). La publication publique du rapport k-anonyme (journal) est
   **déjà** la voie de transparence — un rapport public supplémentaire par
   client n'est pas requis pour la promesse. En attente de validation pilote.

## ③ — N3+ : COUVERTURE PII SÉNÉGALAISE ÉTENDUE (demande pilote en session)

> Nouveaux identifiants à couvrir (une valeur non détectée partirait en clair,
> invariant I1) : **téléphones fixes**, **passeports**, **permis de conduire**,
> **matricules État/IPRES**.

### Formats (recherche publique, 24/08/2026)

- **Téléphones FIXES** (confirmé — Wikipedia « Telephone numbers in
  Senegal ») : préfixes **30** (Expresso), **32** (Tigo/Sentel), **33**
  (Sonatel/Orange), **36** (Hayo/CSU) ; code de zone **8** (Dakar) ou **9**
  (hors Dakar) ; NSN 8 chiffres. Le format international `+221 3X …` était
  déjà couvert (branche `3[0-9]`) ; le format local manquait.
- **Passeport** (CEDEAO/ICAO observé) : 1-2 lettres + 7-8 chiffres —
  structure exacte **non documentée publiquement** → détection
  **contextuelle** (« passeport »/« passport » + numéro).
- **Permis de conduire** : 7-10 chiffres observés — **à confirmer** →
  contextuelle (« permis de conduire » + numéro).
- **Matricule État/IPRES** : 8-11 chiffres observés (fiches de notation du
  ministère de l'Éducation, listes CAP de la Fonction publique) — **à
  confirmer** → contextuelle (« matricule »/« IPRES » + numéro).

Honnêteté (charte §11) : les formats passeport/permis/matricule ne sont pas
confirmés par une source normative publique → la détection est volontairement
**conservatrice** (mot-clé obligatoire à proximité), ce qui évite les faux
positifs massifs tout en couvrant les cas réels.

### Implémentation

- `crates/cloison-core/src/detection.rs` :
  - regex `phone_sn_re` étendue : branche locale fixes
    `(?:30|32|33|36)(?:[89]\d{6}|\s?[89]\s?\d{2}\s?\d{2}\s?\d{2})` ;
  - nouveaux `DetectorKind::Passport` / `DriverLicense` / `Matricule` avec
    regex contextuelles (capture du numéro seul, pas du mot-clé) ;
  - `detect_all`/`detect_with_policy` branchés ;
  - **+5 tests** : fixes 33/30 (7 variantes + anti-faux-positif), passeport
    (span numéro seul + anti-faux-positif), permis, matricule.
- `token.rs` : tags `PP` (passeport), `DL` (permis), `MA` (matricule) —
  mapping aller/retour complet.
- `policy.rs` : les trois nouveaux types **masqués par défaut** (invariant I1).
- `bench/cloison-bench/` :
  - `generator.py` : `PREFIXES_TEL_FIXE` + `ZONES_TEL_FIXE` (25 % de fixes
    dans `generate_tel`) ; `generate_passport`/`generate_permis`/
    `generate_matricule` ; templates contextuels simples + `_fill_template`
    étendu ;
  - `presidio_baseline.py` : regex fixes (international/00/local/espacé/
    parenthèses) + `SenegalContextualIDRecognizer` (passeport/permis/
    matricule, **hors grille**) ;
  - `run_detect_target.py` : mapping core → PASSPORT/PERMIS/MATRICULE
    (hors `ENTITY_WEIGHTS` — la grille v1.1 reste **FIGÉE**) ;
  - `test_benchmark.py` : tests fixes (local 8 chiffres, zone 8/9) +
    passeport/permis/matricule ; README à jour.
- `docs/DATA-MODEL.md` : tags PP/DL/MA + note fixes.

### Résultats

- Core : **50 tests verts** (45 existants + 5 nouveaux) + 17 invariants.
- Bench : **32 tests verts** (fixes + PP/DL/MA générés).
- Jeu régénéré (seed 42, 500 docs) : préfixes TEL gold = {30:18, 32:24,
  33:25, 36:21, 70:53, 71:50, 75:51, 76:55, 77:43, 78:63} — **les 10
  préfixes couverts** ; types gold incluent PASSPORT/PERMIS/MATRICULE
  (hors grille, non scorés).
- **GO/NO-GO re-validé** (grille v1.1, baseline officielle 0.7501) :
  torch macro 0.9542 · PERSON 0.9387 · LOC 0.8320 · CNI/MAIL/TEL 1.000 ·
  spécificité 76 % — **5/5 PASS** (les nouveaux types hors grille ne
  dégradent pas la grille, la spécificité reste ≥ 60 %).
- Onnx-int8 : voir résultats ci-dessous (run en cours au moment de l'écriture).

### Porte de sortie N3+

- [x] Fixes 30/32/33/36 détectés (core + baseline + jeu).
- [x] Passeport/permis/matricule détectés (contextuels, masqués par défaut).
- [x] GO re-validé (grille v1.1, baseline officielle) — torch PASS.
- [x] Tests : core 50 + bench 32 + detect 77 (non-régression).
- [ ] Onnx-int8 + redéploiement edge + e2e + re-publication open-core
      (v0.2.1 si applicable) + CI.

## Vérifications finales (porte de sortie N3)

- [x] 71/75 corrigé (core + bench + tests) et **GO re-validé** (grille v1.1,
      modèles réels, torch + onnx vs baseline officielle).
- [x] Edge **redéployé** avec le fix, preuve e2e 71/75 (masquage amont +
      restauration) **SUCCÈS**.
- [x] Open-core **re-publié** (core + bench v0.2.0) et vérifié (pytest/cargo
      test sur les tags).
- [x] `cloison-cli` livré (build/test/clippy/fmt verts).
- [x] Onboarding scripté + documenté ; docs client publiées.
- [x] Journal public restylé (design system) et déployé.
- [x] Stack prod saine (detect healthy, afroxlmr chargé, 401 sans auth,
      ledger 3 lignes, memwatch 0 OOM, certs J-14).
- [x] e2e réel : **voir Résultats** (OpenRouter, clé du fichier SERVEUR).
- [x] **N3+** : fixes 30/32/33/36 + passeport + permis + matricules détectés
      (core + bench + tests), GO torch re-validé (voir §③).
- [ ] Décisions pilote : dsh.wonkom.ai, mode audit public (documentées).

## Invariants de sécurité vérifiés

- Zéro PII réelle : jeux synthétiques (seed 42), e2e avec PII simulée
  (« Aminata », « 71 123 45 67 », email `.sn` fictif).
- Zéro secret : aucun clair `mn_` affiché hors émission (CLI) ; les hash
  partagent le domaine `cloison-mn-token-v1:` (contrôle = proxy = CLI).
- I2 : le CLI `verify` ne transmet que le digest ; le stockage ne contient
  que des hash.
- I9/O2 : journal public = compteurs k-anonymes contresignés, jamais de texte.
- La grille v1.1 n'a pas été modifiée (baseline_ref restaurée, critères intacts ;
  les nouvelles entités N3+ sont mesurées, non scorées — hors `ENTITY_WEIGHTS`).
- Les formats passeport/permis/matricule sont documentés « à confirmer » et
  détectés de façon **contextuelle** (charte §11 : périmètre honnête).

## Dette / suite

- Attribution opérateur 71/75 : à confirmer (ARTP) — documentation.
- Formats passeport / permis / matricule État-IPRES : **à confirmer auprès
  de sources normatives** (structure observée, détection contextuelle) —
  re-valider la couverture quand les formats officiels seront disponibles.
- Deps bench non-épinglées (dérive baseline) : à pinner pour la
  reproductibilité (dette secondaire REPRISE-DEPLOIEMENT §6bis).
- GPU (dette ②) : en attente (aucun GPU ; baseline ONNX de DEPLOY-8 comme
  référence).
- **N0** (kit léger Rust seul, daemon desktop) : session ultérieure,
  design posé (§6bis) — prérequis 71/75 réglé ✅.
- `cloison-cli` : re-publication open-core v0.2.0 **FAITE** (dépôt public
  `coucagog/cloison-cli`, manifest autonome, git deps épinglées v0.1.0,
  cargo test 4 PASS sur le tag) — le dépôt n'est plus un squelette.
- Re-publication open-core N3+ (core/bench v0.2.1) : **FAITE** — core 50+17,
  bench 36/36, README relu (verdict GO actualisé). `cloison-cli` v0.2.0
  également publié (logique N3 + fix ledger).
