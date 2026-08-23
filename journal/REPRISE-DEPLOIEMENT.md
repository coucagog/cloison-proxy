# CLOISON — REPRISE-DEPLOIEMENT (handoff pour la prochaine session)

> Écrit à la fin de la campagne DEPLOY-1→6 (VPS 144.217.81.251), mis à jour
> après DEPLOY-5 (wiring C) et DEPLOY-6 (torch 2.6.0 + fix NER africain). À
> lire EN PREMIER par toute session qui reprend le déploiement. Complète
> `journal/REPRISE.md` (handoff produit) et `journal/DEPLOY-*.md`.

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

## 6. Prochaine session — DETTES prioritaires (ordre validé par le pilote)

### ~~① Publication open-core PUBLIQUE~~ → **RÉSOLUE (DEPLOY-7, 23 août 2026)**

**Publié** : 10 dépôts publics `coucagog/cloison-{core,proxy,audit,control,
ledger,verify,cli,wasm,detect,bench}` (branche `main` + tag `v0.1.0`),
extraits par `git subtree split` de la source de vérité (`4ed0c45`), adaptés
(Cargo.toml autonome, **git deps épinglées** `tag = "v0.1.0"`, licences,
README), vérifiés deux fois (workspace path-deps puis dépôts publiés avec git
deps réelles — tout vert), `Cargo.lock` verrouillés. Portes : 0 secret / 0 PII
dans l'historique complet ; `cloison-corpus` jamais publié. Détails :
`journal/DEPLOY-7.md`, `docs/OPEN-CORE.md` §4.

### ② GPU
**Objectif** : réduire la latence detect (mesurée ~0,5 s court / ~1,7 s
160 mots sur CPU) à ~50-150 ms/doc.
**Démarrage** : sizing documenté dans `journal/DEPLOY-6.md` — carte d'entrée
~2-4 Go VRAM (T4/L4 ou équivalent cloud), afroxlmr-large en fp16 (ou int8
~1 Go) ; brancher `torch.cuda`/`device="cuda"` dans `african_models.py` +
`gliner_detect.py` (config `CLOISON_DEVICE` à ajouter), re-mesurer, re-valider
GO (précision fp16/int8). Le verdict GO ne requiert pas le GPU — c'est une
décision d'infrastructure (achat/cloud) + d'ingénierie.
**Si pas de GPU** : passer à la dette ③ (ONNX) qui est l'optimisation CPU.

### ~~③ Priorisation de la voie ONNX~~ → **RÉSOLUE (DEPLOY-8, 23 août 2026)**

**Implémenté et validé** : `CLOISON_ONNX` câblée (config → code → tests → docs),
inférence du NER africain (afroxlmr) via ONNX Runtime CPU (int8 dynamique,
repli fp32, fallback torch), export au premier chargement (dynamic axes) —
**GO re-validé sur le chemin ONNX** (macro 0.9546, PERSON 0.9380, LOC 0.8351,
spéc 0.77 — écart int8 vs torch négligeable), latence doc moyen ~20-25 %.
GLiNER reste en torch (pas d'export ONNX dans gliner 0.2.12 — décision
documentée). Dépôt public `cloison-detect` re-publié v0.2.0. Détails :
`journal/DEPLOY-8.md`.

## 6bis. Secondaires (si le temps le permet)

- **Épingler les deps bench** (`bench/cloison-bench/requirements.txt` :
  presidio/spacy/numpy en `>=`) — la baseline régénérée a dérivé
  (macro 0.7501 → 0.7623, spécificité 0.42 → 0.54) à cause de presidio plus
  récent ; la référence officielle 0.7501 reste gravée dans `rapport.json`.
  Pinner pour la reproductibilité.
- **Latence sous charge** — le modèle partagé sérialise les requêtes
  (verrou) : pool d'inférence ou batching par lot si la charge augmente.
- **CI** — le run `cf2d0c6` est vert (9/9, torch 2.6.0, e2e LLM réel) ;
  surveiller le prochain push ; secrets GitHub e2e posés.
- **Hygiène** — `CLOISON_ONNX` mort : implémenter (dette ③) ou retirer la
  variable de la config/compose/docs pour ne pas mentir.

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

**Priorité proposée** : avant ou avec la dette ③ (ONNX) — même zone (détection).

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
