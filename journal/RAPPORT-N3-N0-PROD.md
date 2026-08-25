# CLOISON — RAPPORT D'ÉTAT N3 / N0 & PRÉPARATION DE LA PROCHAINE SESSION

> Écrit le 26/08/2026 (fin de session STACK-N0V12 / open-core v0.2.5).
> Destinataires : pilote (MLS), agents de dev futurs. À lire avec
> `journal/REPRISE.md` + `journal/REPRISE-DEPLOIEMENT.md` (handoffs vivants)
> et la charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§4 échelle N0–N3).
> Ce document est **l'état de vérité au 26/08** : ce qui est FAIT (avec
> preuves), ce qui RESTE avant production (avec critère de sortie), et le
> FONCTIONNEMENT ATTENDU de chaque déclinaison.

---

## 1. Vue d'ensemble — où on en est

Le produit CLOISON (proxy de confidentialité PII compatible OpenAI) est
**fonctionnel et prouvé sur ses deux déclinaisons** :

| | **N3 — hébergé** (`api.wonkom.ai`) | **N0 — local** (daemon desktop) |
|---|---|---|
| Moteur | core Rust + sidecar NER Python (Presidio/GLiNER/afroxlmr) | **core Rust seul** + NER léger embarqué ONNX (distilbert) |
| Déployé | ✅ VPS 144.217.81.251, TLS, 5 conteneurs | ✅ binaire + coffre local (code), script d'install |
| Verdict GO | ✅ grille v1.1 (5/5, baseline 0.7501, torch + onnx) | ✅ arbitrage ④ (STACK-N0V12) |
| Open-core | ✅ 10 dépôts publics, v0.2.5 cascade | ✅ (même core/proxy publiés) |
| État global | **Pré-production immédiate** (décisions pilote + trafic réel) | **Pré-production** (packaging distributable + tests OS réels) |

Chronologie : STACK-0→8 (produit + GO) → DEPLOY-1→10 (déploiement +
couverture + wiring) → N3 commercial (DEPLOY-9) → N0 v1 / v1.1 / v1.2
(STACK-N0, N0V11, N0V12) → open-core v0.2.5 (26/08).

---

## 2. N3 — Hébergé (couche commerciale)

### 2.1 Ce qui est FAIT (avec preuves)

**Infrastructure (DEPLOY-1→10, VPS 144.217.81.251)**
- Stack docker : `edge` (8787, publié) · `control` (8788, interne) ·
  `detect` (8080/50051, interne, healthy) · `postgres` (interne) ·
  `journal` (127.0.0.1:8789). 5 conteneurs up, **0 OOM** (memwatch), certs
  auto-renouvelés (Caddy, sonde J-14, ZeroSSL secours).
- TLS : `https://api.wonkom.ai` (Let's Encrypt, renouvellement auto,
  agrafage OCSP, zéro log d'Authorization/query — invariant I1).
- CI GitHub **verte** : fmt/clippy/test-rust (+feature pg)/test-detect/bench/
  images ×4 (GHCR + SBOM syft + cosign OIDC + trivy porte)/e2e-llm réel.

**Produit (fonctionnel de bout en bout)**
- Passerelle OpenAI-compatible : `/v1/chat/completions` (non-stream +
  stream SSE buffer-and-scan + tool-calls), `/v1/completions` (legacy),
  `/v1/models`. Auth par clé composite `mn_<jeton>.<clé_amont>`.
- **Wiring C** (DEPLOY-5) : vérification des jetons par **hash** auprès du
  contrôle (le clair ne quitte jamais l'edge, fail-closed), long-poll
  rotation, **ingest automatique** des reçus d'audit → journal de
  transparence public (**3 lignes** : genèse + seq 1 + seq 2).
- Détection : 12 préfixes mobiles (70/71/72/75/76/77/78/79), fixes
  (30/32/33/36), CNI Luhn, MAIL, carte, IP, date, passeport/permis/
  matricule (contextuels, masqués par défaut). Verdict **GO re-validé**
  torch (macro 0.9573) et **onnx-int8** (0.9560) vs baseline officielle
  0.7501 (grille v1.1 FIGÉE).
- **ONNX int8 déployé en prod** (`CLOISON_ONNX=1`, backend vérifié au
  boot) ; `CLOISON_ROLE` lu (fail-loud edge/control) ;
  `CLOISON_DETECT_CONCURRENCY` (protection CPU sous charge).
- Mode audit observe-only (reçus signés Ed25519, k-anonymat, rapport
  `GET /v1/audit/report?period=…`, persistance JSONL 0600).
- PostgresStore (feature `pg`) testé sur la base réelle du VPS (2/2).

**Couche commerciale (DEPLOY-9)**
- `cloison-cli` ops complet : provision / token issue·rotate·revoke·verify
  (par hash local) / policy / license / ledger root·check / stats.
- Onboarding scripté + documenté (`deploy/onboard_client.sh`,
  `docs/ONBOARDING.md`) : tenant → jeton `mn_` (clair affiché UNE fois) →
  clé composite livrée → vérification.
- Docs client (`docs/CLIENT-GUIDE.md`) : 2 champs (Base URL + clé
  composite), FAQ confidentialité, limites honnêtes.
- Journal public `journal.wonkom.ai` : page + `/ledger.jsonl` +
  `/control_pubkey.hex` + WASM `cloison-verify` (vérification dans le
  navigateur), design system + thèmes sombre/clair vérifiés (harnais
  `deploy/theme-check/`).
- Open-core **publié** (10 dépôts publics, licences correctes — proxy
  AGPL-3.0 restauré en v0.2.5, reste Apache-2.0).

**Preuves de bout en bout** : e2e mock anti-pass-through (le faux LLM reçoit
des sentinelles ⟦, JAMAIS la PII — un proxy pass-through échoue) + e2e réel
OpenRouter (nom/téléphone/email restaurés, zéro jeton résiduel).

### 2.2 Fonctionnement attendu (N3)

```
Interface IA (Open WebUI / LibreChat / bolt.diy / agent Hermes)
   │  Base URL : https://api.wonkom.ai/v1
   │  Clé      : mn_<jeton_client>.<clé_amont_du_client>
   ▼
edge (api.wonkom.ai, 8787) ──► detect (interne, NER ouest-africain)
   │  pseudonymise PII en ⟦…⟧ avant l'amont      │  Presidio+GLiNER+afroxlmr
   │  (tokenisation + restauration au retour)     │  (spans validés par le core)
   ▼
Fournisseur LLM (OpenRouter/DeepSeek) — ne reçoit QUE des jetons
   ▼
edge restaure les vraies valeurs → réponse naturelle au client
   │
   └─► control (interne) : vérifie les jetons par hash, contresigne
        les reçus d'audit → ledger public (compteurs k-anonymes)
   └─► journal.wonkom.ai : transparence vérifiable (chaîne + signatures)
```

**Ce que le client voit** : ses données n'atteignent jamais le modèle en
clair ; nous ne les voyons pas (hash + compteurs uniquement) ; la promesse
est vérifiable (journal public + code ouvert + rapport de conformité
k-anonyme en mode audit opt-in).

### 2.3 Ce qui RESTE avant production N3

| # | Item | État | Critère de sortie |
|---|---|---|---|
| N3-1 | **Trafic client réel** | Le ledger n'a que 3 lignes (semis) — aucun client réel n'a encore branché sa clé | 1er client en production ; calibration des seuils sur trafic réel (`measure_clusters.py`) |
| N3-2 | **Calibration fine des seuils** | Procédure documentée, non exécutée sur trafic réel | Seuils GO recalibrés avec le trafic réel, consignés |
| N3-3 | **Latence sous charge réelle** | 10,7 ms/doc NER isolé ; ~0,5 s/doc court, ~1,7 s doc long via sidecar (4 vCPU) ; `CONCURRENCY` livré | Sizing validé sur le pic attendu ; **GPU (dette ②)** si la charge le justifie (décision pilote, baseline ONNX chiffrée) |
| N3-4 | **DNS `dsh.wonkom.ai`** | Record A encore présent (zone anycast.me), rien ne le sert ; décision pilote SOLDÉE (retrait validé) | **Action opérateur** : suppression du record (instruction DEPLOY-10 §« Décision pilote requise ») |
| N3-5 | **Formats passeport / permis** | Détection contextuelle conservée (formats non confirmés par source normative) | Confirmation par source normative (ARTP/CDP) ; re-validation couverture si format officiel trouvé |
| N3-6 | **Durcissement N2/N3** (enclave attestée, hardening hébergé avancé) | Hors périmètre v1 (charte §16) | Décision pilote explicite avant d'entamer |
| N3-7 | **Métriques/observabilité opérationnelles** | Compteurs fail-loud + logs structurés (zéro PII) | Dashboard ops (Prometheus/Grafana, compteurs uniquement) si le pilote le demande |
| N3-8 | **Page produit / pricing / CGV** | Docs techniques + guide client prêts | Décision commerciale pilote (positionnement, tarifs, juridique sénégalais — charte §11) |

**Autres dettes transverses** (non bloquantes N3) : `CLOISON_PRELOAD`/preload
boot (constaté, pas critique), wiring gRPC detect non testé de bout en bout
(REST utilisé en prod), `session_ref_hashed` sur vraie session (déjà
renforcé DEPLOY-5, à confirmer avec l'usage).

---

## 3. N0 — Local (kit moteur léger, daemon desktop)

### 3.1 Ce qui est FAIT (avec preuves)

**STACK-N0 (v1, 25/08/2026)** — moteur **Rust seul**, jamais de sidecar Python :
- Daemon desktop compatible OpenAI : `localhost:8787`, clé composite locale
  `mn_<jeton>.<clé_amont>` ; **auth 100 % locale** (zéro dépendance au plan
  de contrôle pour masquer) ; audit k-anonyme **opt-in**.
- **Coffre persistant** redb chiffré AES-256-GCM, clé dérivée d'une
  passphrase locale (HKDF), **fail-loud** au boot (passphrase absente/
  mauvaise/coffre corrompu → refus de démarrer, jamais de recréation
  silencieuse) ; TTL (défaut 7 j) ; **sel de session persistant** (la
  session survit aux redémarrages).
- Généralisation des faibles cardinalités active (date → `YYYY-MM`, IP →
  `[IP]`, CB → masque, **ville → `[VILLE_SN]`**) — jamais de jeton.
- `/v1/embeddings` **bloqué** (404). Limites honnêtes (`docs/N0.md` §4).
- Preuve e2e locale : roundtrip, coffre sans clair, persistance après
  restart, fail-loud.

**STACK-N0V11 (v1.1)** :
- ① **Alias intra-session R1–R7 in-core** (prénom seul, titre+nom, nom seul
  hors noms communs, diminutifs, raccourcis de lieux, casse/diacritiques —
  **jamais les pronoms**, scores plafonnés) + **jauge quasi-id in-core**
  (signal opt-in, jamais de résolution). Serveur bit-identique.
- ② **Keychain OS** pour la passphrase (`keyring` v3 — Credential Manager /
  Keychain / Secret Service-keyutils), repli env avec warn, fail-loud,
  jamais en clair par CLOISON.
- ③ **Module navigateur `@cloison/core`** (`cloison-wasm` ré-exporte les
  bindings — tokenize/restore in-browser, coffre **in-memory** (0 valeur
  persistée), zéro secret, démo `deploy/wasm-demo/`).

**STACK-N0V12 (v1.2, 26/08/2026)** — **chantier ④, NER léger embarqué** :
- Arbitrage pré-enregistré **GO** (`journal/ARBITRAGE-04-NER-LEGER.md`) :
  PERSON 0 → **0.62**, LOC **+0.18**, spécificité **83 %** (amendement C3
  documenté), latence **10,7 ms**/doc court.
- Candidat : `distilbert-base-multilingual-cased-ner-hrl` ONNX int8
  (135 Mo, licence AFL-3.0 provisionnée — jamais committée) ; inférence via
  ONNX Runtime **Rust** (`ort` 2.0.0-rc.13 `load-dynamic` + `tokenizers`) ;
  **fusion englobante N0** (un nom complet prime sur les fragments
  gazetteer) ; **dégradation gracieuse** (modèle absent → N0 v1, warn).
- **Bug corrigé** : la généralisation ville_sn (`Policy::n0_for`) n'était
  pas appliquée (la ville restait en clair) — `apply_rule` + test.
- Preuve e2e réelle : « Xolani Ndlovu » masqué ⟦PE⟧, téléphone ⟦PH⟧, ville
  `[VILLE_SN]`, restauration complète.
- Portes : **286 tests verts**, clippy 0, fmt 0, WASM ok, e2e_n0 8/8.

**Packaging** : `deploy/install-n0.sh` (build release + `~/.cloison` + clé
locataire affichée une fois) ; `deploy/provision_ner_lite.sh` (export ONNX
int8 + tokenizer + lib — requis UNE fois, avec un venv torch).

### 3.2 Fonctionnement attendu (N0)

```
Interface IA locale (Open WebUI / LibreChat / …)
   │  Base URL : http://localhost:8787/v1
   │  Clé      : mn_<jeton_local>.<clé_amont>
   ▼
daemon N0 (cloison-proxy, moteur Rust SEUL — 127.0.0.1:8787)
   │  détection : regex/gazetteers/Luhn (core) + NER léger embarqué
   │  (distilbert ONNX int8 — PERSON/LOC in-core) + alias intra-session
   │  généralisation des faibles cardinalités + jauge quasi-id (opt-in)
   │  coffre chiffré local (passphrase/keychain, fail-loud)
   ▼
Fournisseur LLM — ne reçoit QUE des jetons (le clair ne quitte PAS le poste)
   ▼
daemon restaure → réponse naturelle
   │  audit k-anonyme OPT-IN uniquement (sinon aucun reçu ne sort)
```

**La promesse N0** : le moteur descend chez le client, l'opérateur ne lit
**rien** — promesse absolue (hors poste compromis, documenté). Le cloud ne
voit que des compteurs opt-in. C'est la **cible v1 de la charte** (§4).

### 3.3 Ce qui RESTE avant production N0

| # | Item | État | Critère de sortie |
|---|---|---|---|
| N0-1 | **Packaging distributable** | Binaire compilable + `install-n0.sh` ; pas de release binaires (Windows/macOS/Linux) ni de mise à jour auto | Releases GitHub par OS (ou installers) + doc d'install grand public |
| N0-2 | **Tests OS réels** | Keychain testé sur Linux (Secret Service) ; Windows Credential Manager / macOS Keychain **non testés sur machines réelles** | CI multi-OS ou tests manuels documentés sur Win/macOS (coffre + keychain + NER) |
| N0-3 | **Provisionnement modèles grand public** | `provision_ner_lite.sh` exige un venv torch UNE fois | Option : binaire autonome qui télécharge le modèle pré-exporté (artefact) OU documenter clairement l'étape |
| N0-4 | **Latence sous charge** | Session ONNX sérialisée par `Mutex` (10,7 ms isolé) | Pool d'inférence si l'usage réel le justifie (optimisation documentée) |
| N0-5 | **IndexedDB chiffré navigateur** | Module ③ volontairement in-memory (0 valeur persistée) | Décision pilote : persistance chiffrée navigateur (clé sans keychain = limite) ou in-memory assumé |
| N0-6 | **Déclinaison mobile** | Piste documentée (même moteur que le navigateur) | Décision pilote (périmètre v1 : desktop suffit) |
| N0-7 | **Onboarding client N0** | Docs `N0.md` + `CLIENT-GUIDE.md` §4bis | Guide grand public (installation en 5 min, dépannage) |

**Note honnête (docs/N0.md §4)** : N0 ne protège pas contre un **poste
compromis** (coffre + clés sur la machine) ; les quasi-identifiants sont
**signalés**, pas résolus ; la PII hallucinée par le LLM reste hors du
périmètre d'un proxy (recherche ouverte, charte §16).

---

## 4. Matrice de production — synthèse

| Domaine | N3 | N0 |
|---|---|---|
| Code + tests | ✅ (286 verts, GO re-validé) | ✅ (286 verts, arbitrage GO) |
| Déploiement | ✅ VPS actif, TLS, e2e réel | ⚠️ binaire OK, pas de release grand public |
| Sécurité invariants | ✅ (I1–I12, I-A1–10, O1–6 testés) | ✅ (core intacts, fail-loud, coffre chiffré) |
| Transparence | ✅ journal public + open-core | ✅ open-core (même code) |
| **Blocant restant** | **Trafic client réel + calibration + DNS opérateur** | **Packaging distributable + tests Win/macOS** |
| Décisions pilote en attente | GPU, dsh DNS (action), formats PP/DL, pricing/CGV | IndexedDB, mobile, modèle pré-exporté |

---

## 5. PRÉPARATION DE LA PROCHAINE SESSION

### 5.1 Objectifs proposés (ordre de valeur)

1. **① Packager N0 pour distribution** (bloquant N0-1/N0-3) : builds
   release par OS (au moins Linux + Windows via CI cross), script
   d'install qui provisionne le modèle pré-exporté sans torch (artefact
   publié ou téléchargement direct), doc d'installation grand public.
   → Livrable : un humain installe N0 en ≤ 10 min.
2. **② Premier client N3 réel + calibration** (bloquant N3-1/N3-2) :
   onboarding d'un tenant réel (ou simulateur de trafic représentatif),
   calibration des seuils sur trafic réel (`measure_clusters.py`),
   vérification du ledger alimenté (seq 4+) et du rapport k-anonyme.
   → Livrable : stack N3 prouvée sous trafic réaliste.
3. **③ Décisions pilote à solder** : GPU (dette ② — baseline ONNX
   chiffrée comme référence), DNS `dsh.wonkom.ai` (action opérateur à
   relancer), IndexedDB navigateur, formats passeport/permis (recherche
   normative), déclinaison mobile.
4. **④ Tests OS réels N0** (Windows/macOS : keychain + coffre + NER) —
   si une machine est disponible ; sinon CI multi-OS (builds + tests
   unitaires, hors keychain GUI).

### 5.2 Ordre recommandé et dépendances

- ① **d'abord** (indépendant, livrable concret, débloque la vente N0).
- ② ensuite (nécessite un client ou un accord pilote sur un simulateur —
  coordonner avec le pilote).
- ③ et ④ en parallèle si le temps le permet (③ = décisions + actions
  opérateur ; ④ = validation produit).

### 5.3 Prérequis (vérifiés au 26/08/2026)

- CI GitHub verte (dernier run : ✅) ; stack VPS saine (5 conteneurs,
  ledger 3 lignes, 0 OOM) ; open-core v0.2.5 publié et vérifié.
- Outillage VPS : rustdev (rust 1.97, cible wasm32), onnxdev (torch
  2.6.0+cpu, onnxruntime 1.29), volume `/models` (afroxlmr-onnx,
  ner-lite-distil), scripts `oc-*.sh` (publication open-core).
- Le clone local Windows est synchronisé au commit `8bd9a8f`.
- Décisions pilote soldées : mode audit interne par défaut ✅ ; retrait
  DNS `dsh.wonkom.ai` validé ✅ (action opérateur en attente).

### 5.4 Sortie attendue de la prochaine session

- **N0 distribué** : release binaires + install sans torch + guide
  grand public + (si OS dispo) preuve keychain Win/macOS.
- **N3 sous trafic réel** : calibration consignée, ledger alimenté,
  rapport k-anonyme vérifiable, latence mesurée sous charge.
- **Décisions pilote actées** (GPU, DNS, IndexedDB, PP/DL, mobile) +
  journal `STACK-N0V13.md` (ou `DEPLOY-11.md`) + push + handoffs à jour.

---

*Rapport d'état établi le 26/08/2026. Toute évolution doit être journalisée
et reflétée dans `REPRISE.md` / `REPRISE-DEPLOIEMENT.md`.*
