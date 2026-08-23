# CLOISON — REPRISE-DEPLOIEMENT (handoff pour la prochaine session)

> Écrit à la fin de la campagne DEPLOY-1→4 (VPS 144.217.81.251). À lire EN
> PREMIER par toute session qui reprend le déploiement. Complète
> `journal/REPRISE.md` (handoff produit) et `journal/DEPLOY-*.md`.

---

## 1. Où est le code / comment accéder au serveur

- **Dépôt** : GitHub `coucagog/cloison` (privé), branche `main`.
- **Serveur** : VPS OVH **144.217.81.251** — user `debian`, sudo NOPASSWD.
  SSH : clé `id_rsa_pii` (passphrase dans le fichier SERVEUR, dossier local
  `SERVEUR/` de la machine Windows) — ACL restreinte à l'utilisateur courant.
- **Le repo vit sur l'hôte** : `/home/debian/Cloison/cloison/` (source de
  vérité du déploiement) ; token GitHub dans `~/.git-credentials` (0600).
- **Compose** : `sudo docker compose --profile db --env-file
  /home/debian/Cloison/cloison/.env -f deploy/docker-compose.dev.yml`.

## 2. État de la stack (vérifié en fin de campagne)

| Service | Rôle | Statut |
|---|---|---|
| `edge` (:8787, publié) | passerelle OpenAI (masquage actif) | ✅ |
| `control` (:8788, interne) | plan de contrôle aveugle, ledger | ✅ |
| `detect` (8080/50051, interne) | NER Presidio+GLiNER+afroxlmr, **modèles pré-chargés** | ✅ healthy |
| `postgres` (interne) | persistance PostgresStore | ✅ healthy |
| `journal` (127.0.0.1:8789) | ledger public lecture seule + WASM verify | ✅ |

- **Endpoints publics** : `https://api.wonkom.ai` (401 sans clé composite =
  auth active) · `https://journal.wonkom.ai` (page + `/ledger.jsonl` +
  `/control_pubkey.hex`).
- **Ledger** : genèse + entrée seq 1 (semis DEPLOY-4, reçus synthétiques).
- **Surveillance** : `memwatch.service` (RAM/swap/OOM → `/home/debian/Cloison/
  memwatch.log`) et `cert-expiry.timer` (alerte J-14) — **0 OOM sur toute la
  campagne**.

## 3. Ce qui a été livré (DEPLOY-1 → 4)

1. **DEPLOY-1** : VPS provisionné (Docker 29.7.2, compose v5.5, swap 12 Go,
   git), stack buildée/démarrée, e2e mock 10/10 + réel 5/5, Caddy + TLS LE
   `api.wonkom.ai`, sonde cert, memwatch en service.
2. **DEPLOY-2** : **B.1 wiring edge→detect** (`CLOISON_DETECT_URL` lu ; spans
   PERSON/LOC fusionnés par `cloison-core::tokenize_with_extra` ; types
   `Person`/`Location`, tags `PE`/`LO`) ; **B.2 préchargement réel au boot** ;
   versions épinglées (transformers 4.46.3, hf_hub 0.26.3, torch 2.5.1+cpu —
   image detect 2,06 Go) ; modèles pré-provisionnés (egress interdit sur le
   réseau interne).
3. **DEPLOY-3** : **C surface publique du journal** — conteneur `journal`
   (nginx RO, WASM `cloison-verify`), clé de contrôle stable +
   `control_pubkey.hex`, ledger 0644, Caddy `journal.wonkom.ai`.
4. **DEPLOY-4** : journal alimenté (pipeline réel edge→ingest, seq 1),
   `CLOISON_AGENT_VERIFY_KEY` transmis au contrôle, **rustfmt normalisé**
   (0 diff), clippy + tests verts.

## 4. Vérifications rapides (reprise)

```bash
cd /home/debian/Cloison/cloison
sudo docker compose --profile db --env-file .env -f deploy/docker-compose.dev.yml ps
curl -s -o /dev/null -w '%{http_code}\n' https://api.wonkom.ai/v1/models      # 401
curl -s https://journal.wonkom.ai/ledger.jsonl | wc -l                        # >= 2
sudo systemctl status memwatch cert-expiry.timer
```

## 5. Prochaine session — chantiers recommandés (par priorité)

1. **Automatiser l'ingest** (le point 1 de la dette DEPLOY-4) : le proxy
   n'envoie pas encore ses reçus d'audit au contrôle (`POST /v1/control/ingest`
   reste manuel ; le proxy ne consomme pas `/v1/control/version` non plus).
   C'est LE chaînon manquant du produit (audit → transparence).
2. **Open-core** (charte §5.1) : publier `cloison-core`/`proxy`/`ledger`/
   `verify` (Apache-2.0) — condition de la promesse « nous ne lisons pas » ;
   trancher AGPL pour la passerelle ; `cloison-corpus` privé.
3. **CI** : vérifier le job `fmt` avec le rustfmt stable courant (le dépôt est
   désormais normalisé 1.97) ; publier les images GHCR (le compose utilise
   encore le build local, pas `ghcr.io/coucagog/*`).
4. **PostgresStore** : exécuter les tests d'intégration PG réels (2/2
   « ignorés sans base » — la base tourne désormais) + vérifier la migration
   `001_init.sql` sur le PG du VPS.
5. **Dette produit** (REPRISE.md) : `session_ref_hashed` sur `request_id` ;
   latence CPU detect (GPU conseillé) ; calibration des seuils en prod
   (`measure_clusters.py`).
6. **Hygiène** : déplacer `download_models.py` (provisionnement modèles) dans
   `deploy/` ; supprimer les artefacts de diagnostic du home serveur
   (`audit_key`, `ingest.json`, scripts *).

## 6. Leçons opérationnelles (à ne pas refaire)

- **Egress interdit** sur `cloison-internal` : tout modèle/dépendance doit
  être provisionné AVANT (volume /models ; pip au build) — jamais au boot.
- **Versions épinglées obligatoires** : les transitifs non-pinnés ont cassé
  gliner (hf_hub 1.x) et rempli le disque (torch CUDA). Toujours pinner +
  torch CPU-only (index PyTorch CPU) sur ce VPS.
- **`sudo` réinitialise l'environnement** : passer les variables compose via
  `sudo env VAR=… docker compose …`, pas `VAR=… sudo docker …`.
- **rustfmt 1.97** : piège connu (virgule après commentaire dans un `match`) —
  après une normalisation, contrôler `cargo fmt --check` PUIS `cargo test`.
- **Documents de référence** : la charte (`Doc_REF/CLOISON-NOTE-TECHNIQUE.md`)
  prime ; toute déviation doit être journalisée.
