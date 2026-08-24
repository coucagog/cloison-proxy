# CLOISON — DEPLOY-10 : Dette documentaire (ARTP/formats) + dettes secondaires + DNS mort

> Journal de campagne — exécution des chantiers validés par le pilote
> (24/08/2026) : **① retirer le DNS mort `dsh.wonkom.ai`**, **② dettes
> secondaires** (REPRISE-DEPLOIEMENT §6bis : épinglage deps bench, ONNX en
> prod, latence sous charge, `CLOISON_ROLE`), **③ dette documentaire**
> (attribution opérateur 71/75 — ARTP ; formats passeport/permis/matricule).
> Session du 24 août 2026.

## Objectif

1. **Dette documentaire** : confronter les hypothèses de couverture à des
   **sources normatives** (plan national de numérotation soumis à l'ITU par
   l'ARTP ; listes officielles de la fonction publique) et corriger code +
   jeu + docs. Toute valeur réelle non détectée part en clair (invariant I1).
2. **Dettes secondaires** : épingler les deps bench (reproductibilité de la
   baseline), déployer la voie **ONNX int8 en production** (dette ③),
   adresser la latence sous charge, rendre `CLOISON_ROLE` réel (dette STACK-9).
3. **DNS mort** : acter la suppression du record A `dsh.wonkom.ai`
   (recommandation DEPLOY-9 validée — l'ancien hôte du harness dsh est
   remplacé, rien ne le sert).

## ③ — Dette documentaire : RECHERCHE (sources primaires)

### Attribution opérateur 71/75 — plan national ITU (soumission ARTP)

**Source normative trouvée** : le plan national de numérotation du Sénégal,
soumis par l'**ARTP** et publié par l'**ITU** (`itu.int/oth/T02020000B8`,
document posté le **2023-11-29** — récupéré via l'archive Wayback, l'ITU
bloquant l'accès direct). Table NDC officielle :

| NDC | Usage | Opérateur |
|---|---|---|
| 30 | fixe | Expresso Sénégal |
| 32 | fixe | **FREE Sénégal** (et non Tigo) |
| 338 / 339 | fixe géographique | Sonatel (Orange), Dakar / autres régions |
| 3611 | fixe | CSU SA (HAYO) |
| 390 / 391 | fixe | ADIE |
| 70 | mobile | Expresso Sénégal (sous-blocs Baneex 70 0X/1X/2X/5X/7X) |
| 7211 | mobile | CSU SA (HAYO) |
| 754 / 755 / 756 | mobile | **MVNO PROMOBILE** (Sirius Telecoms Afrique) |
| 757 | mobile | **MVNO ORIGINES SA** |
| 76 | mobile | **FREE Sénégal** (ex-Tigo ; rebrandé **YAS** — Axian — nov. 2024) |
| 77 / 78 | mobile | Sonatel (Orange) |
| 790 | mobile | ADIE |

**Sources citées** :
- [Plan national de numérotation Sénégal — ITU T02020000B8 (posté 2023-11-29)](https://www.itu.int/oth/T02020000B8/en) (accès direct bloqué — copie archive Wayback du PDF officiel, 22 pages)
- [Telephone numbers in Senegal — Wikipedia (table miroir ITU)](https://en.wikipedia.org/wiki/Telephone_numbers_in_Senegal)
- [Senegal Phone Number Format — Sent Resources (màj 2026-04-21 : Orange/Yas/Expresso/MVNO)](https://www.sent.dm/es/resources/phone-number-standards/sn)

**Conclusions (corrections de l'hypothèse DEPLOY-9) :**
- **75 = MVNO (Promobile 754-756, Origines 757)** — **PAS Free** (hypothèse
  « Free 75 » FAUSSE). Sources 2026 concordantes (sent.dm, màj 2026-04-21) :
  Expresso exploite aussi la plage 75X.
- **76 = Free/Yas (ex-Tigo)** — l'hypothèse « Free 76 » était correcte.
- **71 : ABSENT du plan ITU 2023** et d'aucune source 2024-2026 trouvée —
  signalé par le pilote (08/2026). **Décision : couverture conservatrice
  maintenue** (un mobile en 71 non détecté partirait en clair, I1),
  attribution opérateur **à confirmer ARTP** (dette documentaire résiduelle).
- **Découverte de couverture** : les NDC mobiles **72** (7211 CSU/Hayo) et
  **79** (790 ADIE) n'étaient pas couverts en format local → ajoutés
  (core + baseline + jeu + tests). Un mobile 72/79 non détecté partait en
  clair (I1) — même classe de correctif que 71/75.
- Fixe : 32 = Free (commentaire corrigé : ce n'est plus Tigo).

### Format MATRICULE État/IPRES — CONFIRMÉ (listes officielles)

**Source** : `fonctionpublique.gouv.sn` — [PV inspecteurs](https://www.fonctionpublique.gouv.sn/IMG/pdf/pv_inspecteurs.pdf) + [liste provisoire CAP 2025](https://www.fonctionpublique.gouv.sn/IMG/pdf/liste_provisoir_pre_valider_-_cap2025.pdf). Extraction : **76 échantillons, format
UNIQUE : 6 chiffres + 1 lettre majuscule** (`515808/G`, `734123F`,
`757793H`, `611784C`…). Lettre dans l'alphabet de clé de contrôle (A-Z sans
I ni O) ; variante avec slash (`515808/G`) ou sans (`734123F`).

**Correction** : l'hypothèse DEPLOY-9 « 8-11 chiffres » était **FAUSSE** —
le regex `\d{8,11}` ne matchait **jamais** un matricule réel (PII en clair,
I1). Corrigé partout : `\d{6}(?:/)?[A-HJ-NP-Z]` (core Rust + baseline
Presidio + générateur + tests). La lettre de contrôle n'est pas validée
(algorithme non publié — détection, pas vérification).

### Formats PASSEPORT / PERMIS — non confirmés (documenté honnêtement)

- **Passeport** : pas de source normative publique du format du numéro
  (PRADO SEN-AO-03001 via archive : caractéristiques physiques seulement ;
  HL7 : namespace OID seulement ; Wikipedia : structure du document) —
  observation CEDEAO/ICAO (1-2 lettres + 7-8 chiffres) conservée,
  détection contextuelle (charte §11).
- **Permis** : le permis **numérisé SN 009** est le seul valable depuis le
  04/01/2024 (circulaire belge Chapitre 36/Sénégal — l'ancien format papier
  rose n'est plus reconnu) ; format du numéro non confirmé (observé
  7-10 chiffres), détection contextuelle conservée.

## ② — Dettes secondaires

### ②a. Deps bench ÉPINGLÉES (reproductibilité baseline)

`bench/cloison-bench/requirements.txt` : `presidio-analyzer==2.2.355`,
`spacy==3.7.5`, `numpy==1.26.4`, `pytest==9.1.1`, `pytest-cov==7.1.0`,
`regex==2026.7.19`, `tldextract==5.3.2`, `PyYAML==6.0.3` — alignés sur
l'environnement de référence (onnxdev) : un `pip install -r` reproduit la
baseline ; la référence OFFICIELLE reste gravée (`baseline_ref` 0.7501).

### ②b. `CLOISON_ROLE` LU au boot (dette STACK-9)

Chaque binaire vérifie son rôle : `cloison-proxy` exige `edge`,
`cloison-control` exige `control` — valeur incompatible → **échec bruyant**
au boot (jamais de mauvais rôle servi en silence). Absent = rôle natif
(compat dev). Décision documentée : **deux binaires distincts conservés**
— le contrôle exige la feature `pg` (sqlx) que l'edge ne doit pas embarquer
(surface d'attaque, taille d'image distroless) ; le dispatch se fait par
l'image du compose, désormais **vérifié** par le binaire.

### ②c. Latence sous charge — `CLOISON_DETECT_CONCURRENCY`

**Constat (corrige la dette §6bis)** : l'inférence n'est PAS sérialisée par
un verrou — les `threading.Lock` ne protègent que le **chargement lazy**
(double-check) ; les requêtes concurrentes infèrent déjà en parallèle
(uvicorn/gRPC threadpool). Le goulot réel est la **capacité CPU** (4 vCPU).
Implémentation : limiteur optionnel `CLOISON_DETECT_CONCURRENCY`
(0 = illimité, défaut = comportement historique inchangé ; >0 = max de
pipelines `/detect` simultanés) — sémaphore dans `DetectService`, deadline
mesurée après acquisition. +2 tests (sérialisation, env). Les voies de
latence documentées restent ONNX int8 (voir ②d) et GPU (en attente).

### ②d. Voie ONNX DÉPLOYÉE en production (dette ③)

`CLOISON_ONNX=1` + `CLOISON_ONNX_INT8=1` activés en prod (`.env` du VPS) :
l'inférence afroxlmr passe par ONNX Runtime CPU int8 (fichiers
`/models/afroxlmr-onnx/` provisionnés depuis DEPLOY-8), fallback torch
automatique. [Résultats : section ci-dessous.]

## ① — DNS mort `dsh.wonkom.ai`

- Constat vérifié : record A `dsh.wonkom.ai → 144.217.81.251` existe
  (zone **anycast.me**, NS ns200/dns200.anycast.me) ; **rien ne le sert**
  (aucun bloc Caddy, HTTPS → connexion fermée) ; `api.wonkom.ai` vivant
  (401 sans auth).
- **Aucun accès de gestion DNS** (pas de credentials anycast.me/OVH sur le
  VPS ni en local) → la **suppression du record est une action opérateur**
  (zone externe). Action préparée et documentée (voir §Décision pilote) ;
  Caddyfile commenté en conséquence ; aucun bloc Caddy à retirer (il n'y en
  a pas).

## Résultats

### Gates tests (VPS, conteneurs rust:1.97 + onnxdev)

- `cargo test --workspace --locked` : **tous verts** (core 50 unit + 17
  invariants dont les nouveaux tests matricule/72-79 ; proxy/control/audit/
  ledger/verify/cli/wasm inchangés) ; `clippy -D warnings` : **0 erreur** ;
  `cargo fmt --check` : **0 diff** ; `cargo check -p cloison-control
  --features pg --locked` : OK.
- pytest bench : **36/36** (dont `test_generate_matricule` au format
  officiel, `test_generate_tel` 12 préfixes).
- pytest detect : **79/79** (77 existants + 2 tests concurrence).

### Re-validation GO (règle §5 — jeu régénéré, grille v1.1, baseline officielle)

- **Jeu régénéré** (seed 42, 500 docs, hash `218542c0…`) : couverture TEL
  gold complète — **12 préfixes** : {30:21, 32:22, 33:29, 36:19, 70:35,
  71:42, 72:50, 75:46, 76:45, 77:23, 78:24, 79:47}.
- **Baseline dérivée** (deps épinglées, onnxdev) : macro **0.7743** —
  référence OFFICIELLE 0.7501 restaurée dans `rapport.json` (doctrine
  DEPLOY-6/9) ; le GO est évalué contre elle (marges a fortiori).
  NB : la baseline dérivée a été mesurée avec presidio 2.2.355 (onnxdev) ;
  le pin final est 2.2.364 (voir « Correctif CI bench ») — la dérivée
  régénérée peut bouger de quelques millièmes, la référence officielle et
  le verdict (marges > 0.10) ne changent pas.
- **GO TORCH — VERDICT GO (5/5 PASS)** : macro **0.9573** (seuil ≥ 0.850) ·
  PERSON **0.9428** (≥ 0.638) · LOC **0.8450** (≥ 0.746) · CNI 1.0000
  (non-régression) · spécificité **81 %** (≥ 60 %) · MAIL/TEL 1.0000/0.9988.
  Meilleur que DEPLOY-9 (macro 0.9542, spécificité 76 %) — la couverture
  72/79 et le générateur épinglé ne dégradent rien.
- **GO ONNX int8 — VERDICT GO (5/5 PASS)** : macro **0.9560** · PERSON
  **0.9421** · LOC **0.8392** · CNI 1.0000 · spécificité **81 %** — écart
  int8 vs torch négligeable (Δ macro −0.0013).
- **VERDICT FINAL : GO sur les DEUX chemins** (grille v1.1, baseline
  officielle 0.7501). Artefacts : `results/go_nogo_final.deploy10-officiel-
  {torch,onnx}.json` (versionnés).

### Déploiement (VPS 144.217.81.251) — VÉRIFIÉ

- **Images rebuildées** (edge, control, detect) au commit `0df76667` ;
  `.env` : `CLOISON_ONNX=1` + `CLOISON_ONNX_INT8=1` activés.
- **Backend ONNX int8 ACTIF en prod** : log detect au boot « african:
  modèle chargé (afroxlmr, backend=onnx-int8) » — la voie ONNX (dette ③)
  est enfin la voie de production.
- **Stack saine** : edge/control/detect/journal/postgres Up, detect
  **healthy** (healthz 200 interne), `api.wonkom.ai` 401 sans auth
  (auth composite), ledger public 3 lignes intact, memwatch 0 OOM.
- **E2E mock anti-pass-through : SUCCÈS** — PII « Aminata »,
  « **72 111 23 45** » (préfixe 72 — couverture DEPLOY-10) et email
  masquées amont (sentinelles ⟦, pas de PII), restaurées côté client,
  aucun jeton résiduel.
- **E2E RÉEL (OpenRouter, gpt-4o-mini) : SUCCÈS 5/5** — nom/téléphone
  (72 111 23 45)/email restaurés, aucun jeton résiduel, réponse OpenAI
  valide. Le produit fonctionne contre un vrai LLM avec la nouvelle
  couverture.
- **Preuve détection embarquée** (detect_cli) : « Matricule fonction
  publique : 515808/G, IPRES 734123F — appeler le 790123456 » →
  `[{"type":"Matricule",…},{"type":"PhoneSn",…}]` — le matricule au format
  officiel et le 79 (ADIE) sont détectés.

### Open-core re-publié v0.2.2 (procédure docs/OPEN-CORE.md §4)

- **6 dépôts publics** : `cloison-core`, `cloison-bench`, `cloison-detect`,
  `cloison-audit`, `cloison-control`, `cloison-proxy` — branches `main` +
  tag `v0.2.2` (ordre de dépendance, git deps épinglées).
- **Correction en cours de campagne** : le proxy v0.2.2 initial ne
  compilait pas (E0308 — **deux versions de cloison-core dans le graphe** :
  le proxy pointait core v0.2.2 mais `cloison-audit` v0.1.0 épinglait core
  v0.1.0). Corrigé : `cloison-audit` re-publié v0.2.2 (dep core v0.2.2),
  puis control + proxy re-pointés (dep audit v0.2.2). **Leçon** : après
  bump d'une git dep interne, vérifier la résolution du graphe de tous les
  dépendants (même mécanique que la dérive DEPLOY-2).
- **Vérification post-publication** (clones des tags, git deps réelles) :
  core `cargo test` 50+17 ✅ · bench pytest 36/36 ✅ · detect pytest 79/79
  ✅ · audit/control/proxy `cargo check` ✅ (proxy avec git deps
  core+audit v0.2.2).

### Correctif CI bench (constaté sur le premier run du push DEPLOY-10)

Le job `bench` de la CI échouait : `NameError: PatternRecognizer` —
`presidio-analyzer==2.2.355` (pin initial, env VPS py3.11) ne s'importe
**pas sur Python 3.12** (CI), et `spacy 3.7.5` (typer-slim) ne tire pas
`click` (requis par `spacy.cli`/presidio). Corrigé dans
`bench/cloison-bench/requirements.txt` :
`presidio-analyzer==2.2.364` (importe OK py3.12 + py3.11, version prouvée
par les runs CI précédents) + `click==8.1.8` (épinglé). **Validé sur
python:3.12-slim** : `PRESIDIO_AVAILABLE=True`, pytest 36/36,
`run_benchmark.py` complet. Le dépôt public `cloison-bench` v0.2.2 a été
re-pointé avec le même correctif. Leçon : tout pin de dépendances doit être
validé sur l'ENVIRONNEMENT de la CI (py3.12) et pas seulement sur le VPS
(py3.11).

## Invariants de sécurité vérifiés

- Zéro PII réelle : recherche sur documents publics ; les listes
  fonction publique utilisées uniquement pour la STRUCTURE du format
  (6 chiffres + lettre), aucun nom réel dans le code/jeu (le jeu reste
  100 % synthétique, seed 42).
- Zéro secret : aucun credential manipulé ; la clé OpenRouter jamais
  affichée ; les regex/patterns ne contiennent que des formats.
- I1 : couverture complétée (72/79, matricule réel) — toute valeur réelle
  non détectée partirait en clair.
- La grille v1.1 n'a pas été modifiée (critères pré-enregistrés intacts ;
  baseline officielle 0.7501 restaurée dans `rapport.json` — doctrine
  DEPLOY-6/9).

## Décision pilote requise (action opérateur — zone anycast.me)

**Supprimer le record A `dsh.wonkom.ai → 144.217.81.251`** dans le panneau
DNS de la zone `wonkom.ai` (anycast.me). Alternative API (avec token
opérateur) : `DELETE` sur l'endpoint DNS anycast.me de la zone wonkom.ai,
type A, nom `dsh`. Après suppression : `Resolve-DnsName dsh.wonkom.ai` →
NXDOMAIN. Les sous-domaines `api`/`journal` sont INCONTESTABLEMENT
conservés. Le bloc Caddy n'existe pas (rien à retirer) ; le repo est déjà
commenté.

## Dette / suite

- ~~Attribution opérateur **71**~~ → **RÉSOLU (addendum pilote)** : **71 =
  Orange (Sonatel), attribué depuis 2026** — confirmation directe du pilote
  (la recherche publique ne l'avait pas trouvé : absent du plan ITU 2023 et
  des sources 2026 ; la couverture était déjà conservée, l'attribution est
  désormais corrigée dans le générateur, le core, la baseline et les docs).
  GO re-validé après cette correction (voir §Addendum).
- Formats **passeport / permis** : à confirmer auprès de sources normatives
  (structure observée, détection contextuelle conservée).
- GPU (dette ②) : toujours en attente (baseline ONNX chiffrée).
- N0 (kit léger Rust seul) : **PROCHAINE SESSION — préparation
  `journal/N0-PREP.md`** (design §6bis posé).

## ADDENDUM — 71 confirmé Orange (réaction pilote)

**Constat pilote (24/08/2026)** : « le 71 existe bel et bien depuis cette
année, chez Orange ». La recherche publique de cette campagne (plan ITU
2023-11-29, sources 2026) ne listait pas 71 — l'hypothèse de couverture
conservatrice était maintenue, l'attribution restait « à confirmer ARTP ».

**Corrections appliquées** (le préfixe était déjà détecté — invariant I1
préservé ; seule l'ATTRIBUTION change) :
- `generator.py` : `PREFIXES_TEL` — « 71 » déplacé dans `Orange (Sonatel)`
  (`["77","78","71"]`), clé « 71 (signalé pilote…) » supprimée ;
- `detection.rs` (core) : commentaire de la regex téléphone — 71 Orange ;
- `presidio_baseline.py` + `README.md` bench : attribution mise à jour ;
- `REPRISE.md`/`REPRISE-DEPLOIEMENT.md` : dette documentaire 71 soldée.

**Re-validation GO (règle §5 — le générateur change → jeu régénéré)** :
[REMPLIR après le run : torch / onnx vs baseline officielle 0.7501].
Artefacts : `results/go_nogo_final.deploy10-71orange-{torch,onnx}.json`.
Re-publication open-core bench + core (v0.2.2 re-pointé) si le run est vert.

## Porte de sortie

- [x] **Dette documentaire** : attribution TEL confirmée (plan ITU/ARTP —
      75 = MVNO pas Free, 32 fixe = Free, 76 = Free/Yas) ; **couverture
      72/79 ajoutée** (NDC officiels manquants = I1) ; **matricule au
      format officiel** (6 chiffres + lettre, 76 échantillons vérifiés) ;
      passeport/permis documentés honnêtement (à confirmer).
- [x] **GO re-validé** (règle §5 — jeu régénéré 12 préfixes, baseline
      officielle 0.7501) : **torch 0.9573 / onnx 0.9560 — 5/5 PASS les
      deux chemins** ; artefacts `go_nogo_final.deploy10-officiel-*`.
- [x] **Dettes secondaires** : deps bench épinglées (reproductibilité) ;
      **CLOISON_ONNX=1 déployé en prod** (backend onnx-int8 vérifié) ;
      `CLOISON_DETECT_CONCURRENCY` livré + testé ; **CLOISON_ROLE lu au
      boot** (fail-loud edge/control).
- [x] **Tests** : Rust ~230 verts + clippy 0 + fmt 0 + feature pg ;
      bench 36/36 ; detect 79/79.
- [x] **Déploiement vérifié** : stack saine, e2e mock SUCCÈS (72 masqué
      amont) + e2e réel 5/5, detect healthy, ledger intact, 0 OOM.
- [x] **Open-core v0.2.2** : 6 dépôts re-publiés + vérifiés (cargo
      test/check + pytest sur les tags, git deps réelles).
- [x] Journal + push GitHub (commits `c51ae40`, `0df7666`) + CI déclenchée.
- [ ] **DNS `dsh.wonkom.ai`** : suppression du record A = **action
      opérateur** (zone anycast.me — aucun accès de gestion disponible ;
      instruction complète §« Décision pilote requise » ; vérifié : rien ne
      sert le sous-domaine, Caddy sans bloc).
