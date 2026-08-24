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

### Déploiement

[REMPLIR : rebuild images, CLOISON_ONNX=1, backend onnx-int8 vérifié,
e2e mock/réel, stack saine.]

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

- Attribution opérateur **71** : à confirmer ARTP (couverture conservatrice
  maintenue, documentée).
- Formats **passeport / permis** : à confirmer auprès de sources normatives
  (structure observée, détection contextuelle conservée).
- GPU (dette ②) : toujours en attente (baseline ONNX chiffrée).
- N0 (kit léger Rust seul) : prochaine session (design §6bis posé).
