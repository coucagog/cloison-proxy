# CLOISON — REPRISE-DEPLOIEMENT (handoff pour la prochaine session)

> Écrit à la fin de la campagne DEPLOY-1→6 (VPS 144.217.81.251), mis à jour
> après DEPLOY-5 (wiring C), DEPLOY-6 (torch 2.6.0 + fix NER africain),
> DEPLOY-7 (open-core), DEPLOY-8 (ONNX) et **DEPLOY-9/N3 (couche
> commerciale + dette 71/75 réglée, 24/08/2026)**. À lire EN PREMIER par
> toute session qui reprend le déploiement. Complète `journal/REPRISE.md`
> (handoff produit) et `journal/DEPLOY-*.md`.

---

## 1. Où est le code / comment accéder au serveur

- **Dépôt** : GitHub `coucagog/cloison` (privé), branche `main`.
- **Serveur** : VPS OVH **144.217.81.251** — user `debian`, sudo NOPASSWD.
  SSH : clé `id_rsa_pii` (passphrase dans le fichier SERVEUR, dossier local
  `SERVEUR/` de la machine Windows) — ACL restreinte à l'utilisateur courant.
  Sur Windows, le clone local (`CLOISON_PROJECT/cloison`) est synchronisé par
  bundle git (helper `SERVEUR/ssh-run.ps1` — ssh natif Windows + askpass).
- **Le repo vit sur l'hôte** : `/home/debian/Cloison/cloison/` (source de
  vérité du déploiement) ; token GitHub dans `~/.git-credentials` (0600).
- **Compose** : `sudo docker compose --profile db --env-file
  /home/debian/Cloison/cloison/.env -f deploy/docker-compose.dev.yml`.

## 2. État de la stack (vérifié fin DEPLOY-6)

| Service | Rôle | Statut |
|---|---|---|
| `edge` (:8787, publié) | passerelle OpenAI — masquage actif, **auth par hash via contrôle** (wiring C) | ✅ |
| `control` (:8788, interne) | plan de contrôle aveugle, ledger, **`/v1/control/verify`** | ✅ |
| `detect` (8080/50051, interne) | NER Presidio+GLiNER+**afroxlmr ACTIF (fix DEPLOY-6)** — torch 2.6.0+cpu | ✅ healthy |
| `postgres` (interne) | persistance PostgresStore (tenant `default` + hash du jeton edge) | ✅ healthy |
| `journal` (127.0.0.1:8789) | ledger public lecture seule + WASM verify | ✅ |

- **Endpoints publics** : `https://api.wonkom.ai` (401 sans clé composite) ·
  `https://journal.wonkom.ai` (page + `/ledger.jsonl` +
  `/control_pubkey.hex`).
- **Ledger** : genèse + seq 1 (DEPLOY-4) + **seq 2 auto-ingéré** (preuve du
  pipeline edge→contrôle, DEPLOY-5) — 3 lignes.
- **Wiring C actif** : auth des jetons par `POST /v1/control/verify` (hash
  uniquement, fail-closed), long-poll `/v1/control/version` (rotation),
  **ingest automatique** des reçus d'audit (intervalle 60 s, curseur durable).
- **Images GHCR publiées** : `ghcr.io/coucagog/cloison-{proxy:edge,
  control:latest, detect:latest, journal:latest}` — la CI les reconstruit et
  les pousse à chaque push sur main (SBOM + cosign + scans).
- **CI entièrement verte** (8 jobs) depuis le push `a9ac0b1` : fmt (rustfmt
  1.97 épinglé), clippy, test-rust (+ feature pg), test-detect (71 tests),
  bench, images ×4, **e2e-llm réel** (secrets GitHub posés).
- **Surveillance** : `memwatch.service` → `/home/debian/Cloison/memwatch.log`
  (0 OOM sur toute la campagne) ; `cert-expiry.timer` (alerte J-14).

## 3. Ce qui a été livré (DEPLOY-1 → 6)

1. **DEPLOY-1** : VPS provisionné, stack buildée, e2e mock 10/10 + réel 5/5,
   Caddy + TLS `api.wonkom.ai`, memwatch + sonde cert.
2. **DEPLOY-2** : B.1 wiring edge→detect (`CLOISON_DETECT_URL`), B.2 preload
   réel au boot, versions épinglées (transformers 4.46.3, hf_hub 0.26.3,
   torch 2.5.1+cpu), modèles pré-provisionnés (pas d'egress interne).
3. **DEPLOY-3** : C surface publique du journal (`journal.wonkom.ai`, WASM
   `cloison-verify`, clé de contrôle stable).
4. **DEPLOY-4** : journal alimenté (seq 1), `CLOISON_AGENT_VERIFY_KEY`
   transmis, rustfmt normalisé (1.97).
5. **DEPLOY-5** : **wiring C complet** — `ControlClient`/`TokenVerifier`
   (proxy), `POST /v1/control/verify` + `Store::validate_token_hash`
   (contrôle), ingest automatique avec curseur durable, `session_ref_hashed` =
   hash du jeton d'accès, open-core (AGPL passerelle + `docs/OPEN-CORE.md`),
   CI réparée (toolchain 1.97, journal au GHCR, feature pg, httpx, index CPU
   torch, cosign), porte de scan réaliste, `provision_control.sh` +
   `download_models.py` → deploy/, PostgresStore testé sur PG réel (2/2).
6. **DEPLOY-6** : **torch 2.5.1→2.6.0+cpu (CVE-2025-32434)** — GO/NO-GO
   re-validé (PERSON 0.9365 · LOC 0.8366 · macro 0.9546 · spécificité 0.77 —
   **VERDICT GO**), **fix critique NER africain** (voir §5.1), calibration
   exécutée (1218 TP, 0 FP mono-source), latence mesurée (~0,5 s court,
   ~1,7 s 160 mots), GPU/ONNX recommandés et documentés.

## 4. Vérifications rapides (reprise)

```bash
cd /home/debian/Cloison/cloison
sudo docker compose --profile db --env-file .env -f deploy/docker-compose.dev.yml ps
curl -s -o /dev/null -w '%{http_code}\n' https://api.wonkom.ai/v1/models      # 401 sans auth
curl -s https://journal.wonkom.ai/ledger.jsonl | wc -l                        # 3
sudo docker logs cloison-dev-detect-1 2>&1 | grep "african: modèle chargé"   # NER actif
sudo systemctl status memwatch cert-expiry.timer
```

## 5. 🔴 Leçon critique à connaître (DEPLOY-6)

**Le NER africain renvoyait `[]` en production depuis le pin transformers
4.46.3 (DEPLOY-2)** : `african_models.py` passait `offset_mapping` à
`model(**encoded)` — `XLMRobertaForTokenClassification.forward()` n'accepte
pas ce kwarg → TypeError silencieuse (warn + spans ignorés). Le venv
**non-pinné** du STACK-8 tolérait le kwarg (transformers plus récent), d'où le
verdict GO mesuré ; le pin l'a cassé en production. **Corrigé DEPLOY-6**
(`offsets = encoded.pop("offset_mapping")`) + test de régression (71/71).

➡️ **Règle : toute évolution du stack detect (torch/transformers/seuils)
impose de re-valider avec les VRAIS modèles** (pytest ne couvre pas le chemin
réel) : `bench/cloison-bench` + `measure_clusters.py` sur le VPS (le GO run a
attrapé le bug que les 70 tests ne voyaient pas).

## 6. Prochaine session — FINALISER N3 + dette 71/75 + couverture étendue (ordre validé par le pilote, 23/08/2026)

> **✅ TERMINÉ (DEPLOY-9, 24/08/2026)** : dette 71/75 réglée (fix core+bench,
> GO re-validé grille v1.1 torch/onnx vs baseline officielle, edge redéployé,
> preuve e2e mock + réel avec 71, open-core core+bench v0.2.0 republiés) ;
> **N3 livré** (`cloison-cli` ops complet, onboarding scripté + documenté,
> docs client, rapport de conformité, journal public restylé design system) ;
> **N3+ couverture étendue** (fixes 30/32/33/36, passeport PP, permis DL,
> matricules État/IPRES MA — GO re-validé torch 0.9542 / onnx 0.9520,
> open-core v0.2.1). Détails : `journal/DEPLOY-9.md`.
> **✅ TERMINÉ (DEPLOY-10, 24/08/2026)** : **dette documentaire** (attribution
> TEL confirmée plan ITU/ARTP — 75 = MVNO pas Free ; 72/79 ajoutés ;
> matricule au format officiel 6 chiffres+lettre ; PP/DL documentés),
> **dettes secondaires** (deps bench épinglées, CLOISON_ONNX=1 déployé,
> CLOISON_DETECT_CONCURRENCY, CLOISON_ROLE lu au boot), **DNS mort**
> (dsh.wonkom.ai — suppression record = action opérateur anycast.me
> documentée, **décision pilote soldée 25/08/2026 : retrait validé**).
> GO re-validé (torch 0.9573 / onnx 0.9560 vs baseline
> officielle 0.7501 — 5/5 PASS sur les DEUX chemins). Détails :
> `journal/DEPLOY-10.md`.
> **✅ DÉCISIONS PILOTE SOLDÉES (25/08/2026)** : `dsh.wonkom.ai` (retrait DNS
> validé, action opérateur en attente) ; mode audit public vs interne
> (**interne par défaut validé**).
> **✅ N0 LIVRÉ (STACK-N0, 25/08/2026)** : kit moteur **Rust seul** — daemon
> desktop `localhost:8787`, coffre persistant (passphrase locale, fail-loud),
> auth 100 % locale, sel de session persistant, `/v1/embeddings` bloqué,
> limites honnêtes (`docs/N0.md`) ; portes vertes (tests/clippy/fmt/WASM/
> invariants 17) + preuve daemon réel + open-core **v0.2.3** (core/audit/proxy).
> **✅ N0 v1.1 chantier ① (STACK-N0V11)** : alias intra-session R1–R7 +
> jauge quasi-id **in-core** livrés (serveur bit-identique).
> **✅ N0 v1.1 chantier ② (STACK-N0V11)** : **keychain OS** pour la passphrase
> du coffre (`keyring` v3, repli env avec warn, fail-loud, jamais en clair).
> **✅ N0 v1.1 chantier ③ (STACK-N0V11)** : module navigateur `@cloison/core`
> (`cloison-wasm` ré-exporte les bindings, coffre in-memory, zéro secret,
> page de démo `deploy/wasm-demo/`).
> **✅ N0 v1.2 chantier ④ (STACK-N0V12, 26/08/2026)** : **NER léger
> embarqué** — arbitrage pré-enregistré **GO** (`ARBITRAGE-04-NER-LEGER.md`) :
> distilbert HRL ONNX int8 (135 Mo, provisionné — jamais committé) détecte
> PERSON/LOC **in-core** (`ort` 2.0.0-rc.13 load-dynamic + `tokenizers`,
> jamais un sidecar Python), fusion englobante N0, **bug corrigé** (la
> généralisation ville_sn de `Policy::n0_for` n'était pas appliquée).
> Mesures STACK-1 : PERSON 0 → 0.62, LOC +0.18, spécificité 83 % (amendement
> C3 documenté : FP = toponymes réels du jeu, tension STACK-8), latence
> ~11 ms/doc. Dégradation gracieuse (N0 v1 inchangé si modèle absent).
> **Open-core v0.2.5 publié EN CASCADE** (core → audit → proxy + wasm,
> leçon DEPLOY-10 — Cargo.lock épinglés) + **licence proxy AGPL corrigée**
> (régression v0.2.x : `LICENSE` écrasé par l'Apache du workspace ; texte
> GNU restauré, commit `67203b2`). Portes : 286 tests, clippy 0, fmt 0,
> WASM ok, preuve e2e réelle.
> **SESSION 27/08/2026 (①② livrés — `journal/STACK-N0V13.md` +
> `DEPLOY-11.md`)** : **① packaging N0** — distribution **PUBLIQUE**
> `coucagog/cloison-proxy` **v0.3.0** (le monorepo est privé → 404 public ;
> découverte structurante) : 9 assets (4 binaires testés OS réel + bundle NER
> ONNX int8 AFL-3.0 + libs onnxruntime 1.29.0 + checksums), téléchargements
> **200 sans auth**, install `install-n0.sh`/`.ps1` (sans Rust ni torch),
> **smoke Windows réel SUCCÈS** (masquage + NER + coffre), **portage Windows
> du proxy** (`fsperm.rs` — E0599/E0433 découverts par `release-n0`),
> CI `test-n0-os` (Windows/macOS) + `release-n0` verts, open-core proxy
> v0.3.0 re-publié (cargo check vert, AGPL, lock). **② premier client N3** —
> tenant `client-demo` + simulateur 484 requêtes synthétiques (0 PII),
> **ledger 3→13 lignes (seq 12)**, rapport k-anonyme **publiable** (redaction
> < k prouvée : DriverLicense 1→0), chaîne vérifiée `ok=true` 13 entrées,
> calibration 1218 TP / **0 FP mono-source** (consensus tient). **Dettes
> découvertes** : ~~auth edge **mono-tenant**~~ → **SOLDÉE (v0.3.1)** :
> header `X-Cloison-Tenant` (charte §7.2) route la vérification par hash,
> cache par tenant, reçus tagués au tenant, ingest groupé par tenant —
> déployé et **prouvé en production** (jeton client-demo + header → 200 ;
> mauvais tenant / sans header → 401) ; **image edge périmée si `up -d`
> sans `--build`** ; docs CLI (`token-issue` plat). **③ (27/08, décisions
> pilote)** : **GPU = sans** (ONNX CPU baseline, dette close) ; **DNS dsh =
> clos** (wildcard `*.wonkom.ai` du FAI — vérifié 27/08, aucune suppression
> nécessaire, rien ne sert dsh) ; **mobile = GO Android d'abord** (iOS plus
> tard — périmètre v1 à confirmer : app WebView + moteur WASM) ; formats
> PP/DL : recherche 2026 — aucune source normative publique, détection
> contextuelle conservée ; **IndexedDB = choix en attente** (recommandation
> in-memory). **⚠️ Panne d'infrastructure GitHub Actions** (depuis
> 25/08 19:52, « pas de runner » sur tous les jobs) : la distribution binaire
> **v0.3.1** (multi-tenant) est en cours — linux binaire construit et uploadé
> (draft), checksums régénérés ; Windows/macOS en attente de la reprise des
> runners, puis transfert + publication.
> **Prochaine session : dettes transverses** (calibration seuils prod,
> remplacement binaires macOS CI à la reprise des runners) + **déclinaison
> mobile Android** (périmètre v1 à acter) — voir `journal/STACK-N0V12.md` ;
> **GPU : décision pilote = sans** (27/08, dette close ; baseline ONNX).
> **📋 NOUVEAU (28/08, demande pilote) : documentation complète sur le site —
> SLUG ACTÉ : `docs.wonkom.ai`** — **✅ LIVRÉE ET DÉPLOYÉE (29/08,
> `journal/STACK-N0V14.md`)** : `https://docs.wonkom.ai` (8 pages statiques,
> design system du journal, **Caddy file_server — zéro conteneur, zéro log**,
> TLS ACME identique api/journal) ; déploiement `deploy/deploy-docs.sh`
> (idempotent), toutes les routes 200, non-régression vérifiée.
> **Mobile Android v1 livré en source** (`mobile/android/`, app WebView +
> WASM, coffre in-memory, build APK documenté — SDK Android requis ;
> **APK en attente de la reprise des runners GitHub — panne TOUJOURS en
> cours 29/08** : aucun run depuis 27/08, jobs échoués sans steps).
> **Binaires macOS v0.3.1** : idem — en attente de la reprise des runners.

### ① Dette 71/75 — ✅ RÉSOLUE (DEPLOY-9 + DEPLOY-10)

Inventaire corrigé : `detection.rs` (71 ajouté au format local), `presidio_baseline.py`
(5 regex), `generator.py` (71/75, attribution opérateur à confirmer ARTP — commenté),
README bench, test_benchmark. GO re-validé (torch 0.9550 / onnx 0.9556 vs baseline
officielle 0.7501), edge redéployé, preuve e2e 71/75 mock + réel, open-core v0.2.0.
**DEPLOY-10 (addendum pilote)** : **71 = Orange (Sonatel), attribué depuis 2026**
(confirmé pilote — absent du plan ITU 2023 ; couverture déjà conservée) ; 72/79
ajoutés (plan ITU) ; attribution corrigée (générateur/core/baseline/docs).

### ③ N3+ — couverture PII étendue (✅ LIVRÉE — DEPLOY-9)

- **Téléphones FIXES** : 30/32/33/36 (zone 8/9) intégrés à TEL (core regex +
  baseline + générateur + tests) — le jeu couvre les 10 préfixes.
- **Passeport (PP) / permis (DL) / matricules État-IPRES (MA)** : détection
  **contextuelle** (formats à confirmer — charte §11), masqués par défaut,
  **hors grille GO** (la grille v1.1 reste FIGÉE).
- GO re-validé (torch 0.9542 / onnx 0.9520, 5/5 PASS), edge redéployé
  (e2e fixe 33 PASS, preuve PP/MA via detect_cli), open-core **v0.2.1**
  (core + bench, README à jour). Détails : `journal/DEPLOY-9.md` §③.

### ② N3 — couche commerciale (✅ LIVRÉE — DEPLOY-9)

La stack est fonctionnelle ; il manquait la couche « un client peut nous
acheter » :

1. **`cloison-cli` (squelette)** : remplir l'outillage ops N3 — provisioning
   tenant + jeton `mn_` (enveloppe de `deploy/provision_control.sh`), rotation,
   requêtes ledger, stats. Débloque aussi la re-publication du dépôt public
   `cloison-cli` (publié vide à DEPLOY-7).
2. **Onboarding locataire** : flux bout en bout documenté + scripté —
   création tenant, génération jeton `mn_`, livraison de la **clé composite**
   au client, vérification `POST /v1/control/verify`.
3. **Docs client** : guide « pointer votre interface IA sur `api.wonkom.ai` »
   (2 champs : base URL + clé composite), FAQ confidentialité, liens journal
   public + open-core (la promesse vérifiable).
4. **Métriques / rapport client** : rapport de conformité k-anonyme
   (`GET /v1/audit/report`, mode audit observe-only) présentable au client —
   le ledger comme source de vérité.
5. **Décisions pilote (25/08/2026 — SOLDÉES)** : `dsh.wonkom.ai` → **retrait
   du record A validé** (action opérateur anycast.me, en attente
   d'exécution) ; mode audit → **interne par défaut validé** (rapport
   k-anonyme par tenant en observe-only ; transparence publique = journal).
6. **Vérifications finales N3** : ledger alimenté par du trafic réel, memwatch
   0 OOM, certs J-14, e2e réel, stack stable.

**Porte de sortie N3 — ✅ ATTEINTE (DEPLOY-9)** : onboarding client possible
de bout en bout (tenant → clé composite → requête réelle → journal alimenté) ·
71/75 réglé avec GO re-validé · docs client publiées · `cloison-cli` livré ·
dettes à jour. **2 décisions pilote — SOLDÉES (25/08/2026)** :
`dsh.wonkom.ai` (retrait du record A **validé** — action opérateur zone
anycast.me toujours en attente d'exécution, record encore présent vérifié
25/08/2026) ; mode audit public vs interne (**interne par défaut validé** —
rapport k-anonyme par tenant en observe-only, voie de transparence publique
= le journal).

### Dettes résolues (référence)

- ① open-core public : **RÉSOLUE** (DEPLOY-7) — 10 dépôts `coucagog/cloison-*`,
  git deps épinglées v0.1.0, vérifiés deux fois. Détails : `journal/DEPLOY-7.md`.
- ③ voie ONNX : **RÉSOLUE** (DEPLOY-8) — `CLOISON_ONNX` câblée (ONNX Runtime
  CPU int8 pour afroxlmr), GO re-validé (macro 0.9546), latence doc moyen
  ~20-25 %, `cloison-detect`/`cloison-bench` re-publiés v0.2.0. Détails :
  `journal/DEPLOY-8.md`.
- **Préfixes TEL 71/75** : **RÉSOLUE** (DEPLOY-9) — fix core+bench, GO
  re-validé (torch 0.9550 / onnx 0.9556 vs baseline officielle 0.7501),
  edge redéployé, preuve e2e mock + réel (71), `cloison-core`/`cloison-bench`
  re-publiés **v0.2.0**. Détails : `journal/DEPLOY-9.md`.
- **Couverture PII étendue (N3+)** : **RÉSOLUE** (DEPLOY-9 §③) — téléphones
  **fixes 30/32/33/36** (zone 8/9) intégrés à TEL ; **passeport (PP)**,
  **permis (DL)**, **matricules État/IPRES (MA)** en détection **contextuelle**
  (formats à confirmer, masqués par défaut, hors grille). GO re-validé
  (torch 0.9542 / onnx 0.9520), edge redéployé (e2e fixe 33 PASS), open-core
  **v0.2.1** (core + bench, README à jour). Détails : `journal/DEPLOY-9.md` §③.
- **`cloison-cli` (squelette DEPLOY-7)** : **RÉSOLU** (DEPLOY-9) — ops N3
  complet (provision, token issue/rotate/revoke/verify par hash, policy,
  license, ledger root/check, stats). Re-publication du dépôt public
  `cloison-cli` v0.2.0 **à prévoir** (même mécanique que core/bench).

## 6bis. En attente / sessions suivantes

### GPU (dette ②) — EN ATTENTE (reporté, décision pilote 23/08/2026)

Aucun GPU disponible pour le moment → dette ouverte jusqu'à nouvel ordre. Si
un **GPU local** (dans l'infra) ou une autre approche est envisagé, le sizing
et la procédure sont documentés (DEPLOY-6) ; la **baseline ONNX chiffrée de
DEPLOY-8** sert de référence pour mesurer le gain.

### N0 — kit moteur léger — ✅ LIVRÉ (STACK-N0, 25/08/2026)

Décisions pilote posées (23/08/2026) **exécutées** :

- **N0 le plus léger possible** ✅ : moteur **Rust seul** (core : détection
  structurée + gazetteers), **SANS sidecar NER Python** — quelques Mo,
  offline, mobile-friendly. Limite honnête documentée : rappel PERSON/LOC en
  texte libre réduit (voie ONNX = rampe vers un NER léger plus tard).
- **Kit portable** ✅ (daemon desktop v1) : `cloison-proxy` en mode N0
  (`CLOISON_VAULT_PATH` + `CLOISON_VAULT_PASSPHRASE`), endpoint
  OpenAI-compatible `localhost:8787`, clé composite locale.
- **Décisions techniques exécutées** : coffre **persistant** (clé dérivée
  d'une passphrase locale, perte = **fail-loud** au boot), **auth 100 %
  locale** (jeton `mn_` résolu localement — zéro dépendance au plan de
  contrôle pour masquer ; audit k-anonyme **opt-in**), philosophie
  référencée (invariants, open-core, lien journal, mention « poste
  compromis »).
- **Questions ouvertes tranchées** : surface v1 = **daemon desktop** (reco
  retenue) ; alias intra-session (R1-R7) + jauge quasi-id in-core =
  **✅ LIVRÉS en N0 v1.1 (STACK-N0V11)** — portage in-core du sidecar
  (jamais les pronoms, scores plafonnés, jauge signal-only opt-in, serveur
  bit-identique) ; `/v1/embeddings` = **bloqué par défaut** (404).
- Détails : `journal/STACK-N0.md`, `journal/STACK-N0V11.md`, `docs/N0.md`,
  open-core **v0.2.4** publié et vérifié (core/audit/proxy — deps git
  taguées v0.2.4, cargo test des tags publiés).

### Secondaires (si le temps le permet)

- **Épingler les deps bench** — ✅ **RÉSOLU (DEPLOY-10)** :
  `requirements.txt` épinglé sur l'env de référence (presidio 2.2.355,
  spacy 3.7.5, numpy 1.26.4, pytest 9.1.1, regex, tldextract, PyYAML) ;
  baseline régénérée reproductible (dérivée macro 0.7743 — la référence
  OFFICIELLE 0.7501 reste gravée).
- **Latence sous charge** — ✅ **ADRESSE (DEPLOY-10)** :
  `CLOISON_DETECT_CONCURRENCY` (0 = illimité, défaut) ; constat : les
  verrous ne protègent que le chargement lazy, l'inférence est déjà
  parallèle — le goulot réel est le CPU (ONNX int8 déployé en prod ;
  GPU en attente).
- **`CLOISON_ROLE`** — ✅ **RÉSOLU (DEPLOY-10)** : lu au boot par chaque
  binaire (edge/control), valeur incompatible → échec bruyant.
- **Voie ONNX en prod** — ✅ **DÉPLOYÉE (DEPLOY-10)** : `CLOISON_ONNX=1`
  + `CLOISON_ONNX_INT8=1` dans le `.env` du VPS (backend `onnx-int8`
  vérifié au boot).
- **DNS mort `dsh.wonkom.ai`** — décision pilote : **supprimer le record A**
  (zone anycast.me — action opérateur préparée, DEPLOY-10 §①). **SOLDÉE
  (25/08/2026)** : retrait validé, action opérateur toujours en attente
  (record encore présent, vérifié 25/08/2026).
- **CI** — le run `cf2d0c6` est vert ; surveiller le prochain push (le push
  DEPLOY-10 va déclencher un run complet) ; secrets GitHub e2e posés.

## 6ter. Signalé pilote (23/08/2026) — préfixes téléphoniques sénégalais 71/75

**Constat** : les numéros mobiles sénégalais ont évolué ces derniers mois — les
préfixes **71** et **75** existent désormais. Un numéro en 71/75 non détecté est
une **PII qui part en clair vers le LLM** (invariant I1, charte §6.1) : c'est un
correctif de couverture, pas une amélioration.

**Inventaire exact (état `4ed0c45`) :**

| Fichier | État actuel | À faire |
|---|---|---|
| `crates/cloison-core/src/detection.rs` (l.200) | international `+221/00221 7[0-9]` couvre 71/75 ✅ ; **format local `(?:70\|75\|76\|77\|78)` → 71 MANQUANT** | ajouter `71` |
| `bench/cloison-bench/presidio_baseline.py` (l.140/146/152) | `(?:70\|75\|76\|77\|78)\d{7}` ×3 → **71 MANQUANT** (75 présent) | ajouter `71` |
| `bench/cloison-bench/presidio_baseline.py` (l.158/164) | patterns espacés `[67][078]` → **71/75 absents** | aligner |
| `bench/cloison-bench/generator.py` (l.106-109) | `PREFIXES_TEL = {Orange:[77,78], Free:[76], Expresso:[70]}` → **71/75 absents** (le jeu ne les génère jamais) | ajouter 71/75 (attribution opérateur à confirmer) |
| `bench/cloison-bench/README.md` | table TEL « préfixes 70/76/77/78 » | mettre à jour |

**Répercussions** :
- **2 dépôts PUBLICS concernés** : `coucagog/cloison-core` et
  `coucagog/cloison-bench` → re-publier (commit + tag `v0.2.0`, procédure
  `docs/OPEN-CORE.md` §4) ; `cloison-detect` n'a pas de regex TEL propre (l'oracle
  Presidio par défaut détecte `PHONE_NUMBER` — à confirmer sur 71/75).
- **Re-validation obligatoire** (règle §5) : le jeu benchmark doit inclure 71/75,
  la baseline change → **vérifier que le verdict GO tient toujours** (grille v1.1),
  puis e2e mock/réel.
- **Prod** : l'edge embarque le core → rebuild + redéploiement après la mise à jour.

**Priorité** : correctif de couverture (PII en clair sinon) — **à régler dans la
prochaine session (N3)**, voir §6 ①.

## 7. Leçons opérationnelles (à ne pas refaire)

- **Egress interdit** sur `cloison-internal` : provisionner les modèles AVANT
  (volume `/models`), jamais au boot.
- **Versions épinglées obligatoires** (torch CPU-only + index PyTorch CPU) —
  MAIS un pin peut casser silencieusement un chemin non testé : **toujours
  re-valider avec les vrais modèles** après tout changement de dépendance.
- **`sudo` réinitialise l'environnement** : passer les variables compose via
  `sudo env VAR=… docker compose …`, pas `VAR=… sudo docker …`.
- **rustfmt 1.97 / clippy 1.97 épinglés en CI** : le stable courant dérive
  (lints nouveaux) — la CI est déterministe sur 1.97.
- **Quoting PowerShell→ssh** : toute commande distante complexe passe par un
  script base64 (`echo <b64> | base64 -d | bash`) ou un fichier scp'é —
  les guillemets imbriqués sont perdus par le passage natif PowerShell.
- **Porte de scan** : trivy bloquant (HIGH/CRITICAL corrigeables) sur
  proxy/control ; detect/journal en advisory (écosystèmes tiers) — déviation
  O5 journalisée (DEPLOY-5).
- **Documents de référence** : la charte (`Doc_REF/CLOISON-NOTE-TECHNIQUE.md`)
  prime ; toute déviation doit être journalisée.
