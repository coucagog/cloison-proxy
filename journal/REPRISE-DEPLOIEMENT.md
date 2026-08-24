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
> **Prochaine session = N0** (kit moteur léger Rust seul, design §6bis) ;
> **GPU toujours en attente** (aucun GPU disponible — décision reportée ;
> baseline ONNX de DEPLOY-8 comme référence).

### ① Dette 71/75 — ✅ RÉSOLUE (DEPLOY-9)

Inventaire corrigé : `detection.rs` (71 ajouté au format local), `presidio_baseline.py`
(5 regex), `generator.py` (71/75, attribution opérateur à confirmer ARTP — commenté),
README bench, test_benchmark. GO re-validé (torch 0.9550 / onnx 0.9556 vs baseline
officielle 0.7501), edge redéployé, preuve e2e 71/75 mock + réel, open-core v0.2.0.

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
5. **Décisions en attente (DEPLOY-1)** : `dsh.wonkom.ai` (DNS mort — retirer
   l'enregistrement ou le documenter) ; mode audit public vs interne.
6. **Vérifications finales N3** : ledger alimenté par du trafic réel, memwatch
   0 OOM, certs J-14, e2e réel, stack stable.

**Porte de sortie N3 — ✅ ATTEINTE (DEPLOY-9)** : onboarding client possible
de bout en bout (tenant → clé composite → requête réelle → journal alimenté) ·
71/75 réglé avec GO re-validé · docs client publiées · `cloison-cli` livré ·
dettes à jour. Reste **2 décisions pilote** (documentées DEPLOY-9, non
tranchées) : `dsh.wonkom.ai` (DNS mort — recommandation : retirer
l'enregistrement) ; mode audit public vs interne (choix actuel : interne par
défaut, rapport k-anonyme par tenant en observe-only — la voie de transparence
publique reste le journal).

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

### N0 — kit moteur léger (session ULTÉRIEURE, après N3)

Décisions pilote posées (23/08/2026) :

- **N0 le plus léger possible** : moteur **Rust seul** (core : détection
  structurée + gazetteers), **SANS sidecar NER Python** (charte §4 : le
  sidecar est pour les paliers serveur/enclave) — ~quelques Mo, offline,
  mobile-friendly. Limite honnête à documenter : rappel PERSON/LOC en texte
  libre réduit (voie ONNX = rampe vers un NER léger plus tard).
- **Kit portable** : core en bibliothèque (natif desktop/mobile + WASM
  navigateur) — déclinaisons : **daemon desktop** (endpoint
  OpenAI-compatible `localhost:8787`, réutilise le proxy — **recommandé
  v1**), **mobile** (moteur embarqué — pas de daemon sur iOS), **navigateur**
  (`cloison-wasm`, à écrire — squelette aujourd'hui).
- **Décisions techniques** : coffre **persistant** (clé locale keychain,
  perte = fail-loud), **auth 100 % locale** (jeton `mn_` résolu localement —
  zéro dépendance au plan de contrôle pour masquer ; audit k-anonyme
  **opt-in** vers le journal), philosophie référencée (invariants, open-core,
  lien journal, mention « poste compromis »).
- **Questions ouvertes** : surface v1 (daemon desktop vs app embarquée —
  reco daemon) ; alias intra-session (R1-R7) + jauge quasi-id dans le core v1
  (léger, déterministe) ou report v1.1 documenté.
- **Prérequis** : dette 71/75 réglée AVANT d'embarquer le moteur (session N3).

### Secondaires (si le temps le permet)

- **Épingler les deps bench** (`bench/cloison-bench/requirements.txt` :
  presidio/spacy/numpy en `>=`) — la baseline régénérée a dérivé
  (macro 0.7501 → 0.7623, spécificité 0.42 → 0.54) ; la référence officielle
  0.7501 reste gravée dans `rapport.json`. Pinner pour la reproductibilité.
- **Latence sous charge** — le modèle partagé sérialise les requêtes
  (verrou) : pool d'inférence ou batching par lot si la charge augmente.
- **CI** — le run `cf2d0c6` est vert ; surveiller le prochain push (le push
  ONNX va déclencher un run complet) ; secrets GitHub e2e posés.

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
