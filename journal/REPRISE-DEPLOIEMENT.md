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

## 6. Prochaine session — chantiers recommandés (par priorité)

1. **Open-core : publication publique (décision MLS requise)** —
   `docs/OPEN-CORE.md` §4 : `git subtree split` des crates (core, proxy,
   ledger, verify, audit, control, cli, wasm ; detect ; bench) → dépôts
   publics `coucagog/cloison-*`, licences (proxy AGPL-3.0, reste Apache-2.0),
   vérifier `cargo test` sur chaque sous-arbre (ordre des path deps), tags
   `v0.1.0`. Acte irréversible → validation MLS explicite.
2. **Voie ONNX (optimisation CPU latence)** — `CLOISON_ONNX` est une
   **bascule morte** (config parse mais non câblée) : exporter afroxlmr/GLiNER
   en ONNX + `onnxruntime` CPU int8 (gain attendu ×2-3 sur les docs longs),
   **puis re-valider le GO** (la précision int8 peut décaler les scores).
3. **GPU (décision MLS)** — sizing documenté (DEPLOY-6) : carte ~2-4 Go VRAM
   → afroxlmr à ~50-150 ms/doc (×10-30) ; le verdict GO ne le requiert pas.
4. **Épingler les deps bench** (`bench/cloison-bench/requirements.txt` :
   presidio/spacy/numpy en `>=`) — la baseline régénérée a dérivé
   (macro 0.7501 → 0.7623, spécificité 0.42 → 0.54) à cause de presidio plus
   récent ; la référence officielle 0.7501 reste gravée dans `rapport.json`.
   Pinner pour la reproductibilité.
5. **Latence sous charge** — le modèle partagé sérialise les requêtes
   (verrou) : pool d'inférence ou batching par lot si la charge augmente.
6. **CI** — vérifier le run du prochain push (torch 2.6.0 : test-detect 71
   tests + images detect rebuild) ; les secrets GitHub e2e sont posés.
7. **Hygiène** — `CLOISON_ONNX` mort : implémenter (chantier 2) ou retirer la
   variable de la config/compose/docs pour ne pas mentir.

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
