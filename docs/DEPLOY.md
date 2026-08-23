# CLOISON — Déploiement

> Hôte de dev cible : **wonkom.ai**. Trois images, trois rôles : **edge**
> (passerelle LLM, 8787), **control** (plan de contrôle aveugle, 8788),
> **detect** (sidecar NER, 8080 REST / 50051 gRPC). Référence des variables :
> `docs/CONFIG.md`.

## 1. Pré-requis

**Sur le dépôt (code) :**

1. **TLS amont** ✅ fait : `crates/cloison-proxy/Cargo.toml` — `reqwest`
   compilé avec `rustls-tls` (`features = ["json", "stream", "rustls-tls"]`).
2. **Binaire control** ✅ fait : `crates/cloison-control/src/main.rs`
   (routeur `cloison_control::api::router`, écoute `CLOISON_CONTROL_PORT`
   défaut 8788, persistance `CLOISON_LEDGER_FILE`).
3. **`CLOISON_ROLE`** (edge|control) : dispatch encore à implémenter dans
   `config.rs` + garde-fou uid 0 dans `main.rs` (refus de démarrer en root).
4. **Wiring edge→detect** (`CLOISON_DETECT_URL`) : non lu par le binaire
   actuel ; sans lui, le proxy fonctionne avec ses détecteurs embarqués
   (regex, gazetteers, Luhn) — le sidecar n'est pas consommé.

**Outils (wonkom.ai)** : Docker + compose v2, `helm` (déploiement K8s),
`syft`/`grype` (SBOM/scan, cf. `deploy/sbom.sh`), `curl`, `openssl`.

## 2. Quickstart docker-compose (wonkom.ai)

```bash
cd <racine-du-monorepo>

# 1. Secrets locaux (jamais committés)
cp deploy/.env.example .env
#   éditer : CLOISON_EXPECTED_ACCESS_TOKEN, CLOISON_TENANT_KEY_HEX,
#            OPENROUTER_API_KEY (et optionnel CLOISON_SESSION_SALT_HEX)
#   génération :
#     CLOISON_TENANT_KEY_HEX=$(openssl rand -hex 32)
#     CLOISON_ACCESS_TOKEN=mn_$(openssl rand -hex 16)

# 2. Démarrer (edge + control + detect)
docker compose -f deploy/docker-compose.dev.yml up -d --build

# 3. Vérifier (clé composite : Bearer mn_<token>.<openrouter_key>)
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer mn_${CLOISON_EXPECTED_ACCESS_TOKEN}.${OPENROUTER_API_KEY}"

# 4. Requête de bout en bout (PII simulée -> masquée -> restaurée)
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer mn_${CLOISON_EXPECTED_ACCESS_TOKEN}.${OPENROUTER_API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{"model":"openai/gpt-4o-mini","messages":[{"role":"user","content":"Appelez Aminata Diop au 77 123 45 67 (e2e.cloison@example.com)"}],"stream":false}'

# Arrêt
docker compose -f deploy/docker-compose.dev.yml down -v
```

Notes :
- `control` (binaire STACK-7 committé) est construit par
  `deploy/Dockerfile.control` ; en cas de problème sur un service, l'isoler
  avec `docker compose stop control` pour ne déployer que `edge` + `detect`.
- Réseaux : `cloison-net` (bord — edge, port 8787 publié sur l'hôte) et
  `cloison-internal` (`internal: true` — control/detect/postgres, **aucun**
  port publié, aucun egress ; l'API admin ne sort jamais, cf. THREAT-MODEL
  §3.1). NB : docker ne publie aucun port sur un réseau `internal: true`
  (moby/moby#53256) — c'est pourquoi edge est sur un réseau dédié. Pour du
  debug local, décommenter les lignes `ports:` voulues.
- `CLOISON_AUDIT_MODE=1` par défaut (observe-only, reçus signés) ; passer à
  `0` pour le masquage/restauration (e2e réel).
- Images construites depuis les Dockerfiles locaux ; les tags GHCR
  (`ghcr.io/coucagog/*`) sont la cible de publication CI.

## 3. Helm (Kubernetes)

```bash
# lint + dry-run
helm lint deploy/helm
helm template cloison deploy/helm > /tmp/cloison-rendu.yaml

# installation (secrets en --set, jamais dans values.yaml)
helm install cloison deploy/helm \
  --namespace cloison --create-namespace \
  --set global.tenantKeyHex="$CLOISON_TENANT_KEY_HEX" \
  --set global.accessToken="$CLOISON_ACCESS_TOKEN"

# upgrade / rollback
helm upgrade cloison deploy/helm \
  --set edge.ingress.host=proxy.wonkom.ai
helm rollback cloison <revision>

# désinstallation
helm uninstall cloison -n cloison
```

- **Rôles** : une charte, trois Deployments (edge/control/detect) — le rôle
  est une valeur (`values.yaml`). Désactiver un rôle avec
  `--set <role>.enabled=false`.
- **Sécurité** (déployée par défaut) : `runAsNonRoot`, `runAsUser`
  (65532 ; 10001 detect), `readOnlyRootFilesystem: true`,
  `allowPrivilegeEscalation: false`, `capabilities.drop: ["ALL"]`,
  tmpfs `/tmp`, PVC `/data` (edge/control) et `/models` (detect).
- **Sondes** (opérationnelles par défaut, aucune clé requise) :
  `tcpSocket :8787` (edge — le proxy n'expose **pas** de `/healthz` ; un
  `httpGet` sur `/v1/models` sans Authorization recevrait 401 = sonde KO,
  pod jamais Ready), `httpGet /healthz :8788` (control — implémenté dans
  `cloison_control::api`), `httpGet /healthz :8080` (detect).

## 4. Secrets

- **Recommandé** : `global.existingSecret` — un secret K8s pré-existant
  contenant les clés `tenant-key-hex` et `access-token` (clés attendues par
  `templates/secret.yaml`). Aucun secret n'est alors créé par la charte.
- **Sinon** : la charte crée le secret depuis `--set global.tenantKeyHex=…`
  et `--set global.accessToken=…` (`templates/secret.yaml`, `stringData`).
- La clé fournisseur (`OPENROUTER_API_KEY`) est utilisée **à la volée** dans
  la clé composite côté client/CI — ne pas la mettre dans les valeurs Helm
  (utiliser un secret existant monté en env si nécessaire).
- Règles : `.env` jamais committé (`.gitignore` racine + `deploy/.gitignore`,
  qui ignore aussi `*.pem`/`*.key`/`core.*`) ; `deploy/.env.example`
  contient uniquement des commentaires/vides ; secrets CI = GitHub
  `environment: e2e` (`CLOISON_ACCESS_TOKEN`, `OPENROUTER_API_KEY`,
  `CLOISON_TENANT_KEY_HEX`).

## 5. TLS — Caddy (terminaison au bord)

Recommandé en dev/prod sur wonkom.ai : **Caddy** reverse-proxy devant le
edge (terminaison TLS automatique Let's Encrypt).

```caddyfile
# /etc/caddy/Caddyfile
proxy.wonkom.ai {
    reverse_proxy 127.0.0.1:8787
    encode gzip
}
```

```bash
sudo apt install -y caddy
sudo systemctl enable --now caddy   # TLS automatique via le domaine
```

En K8s, TLS via ingress (`values.edge.ingress` : `enabled: true`, host
`proxy.wonkom.ai`, `className: nginx`, `tls: [{secretName: cloison-edge-tls,
hosts: [proxy.wonkom.ai]}]` — le template `templates/ingress.yaml` n'est
rendu que si `edge.ingress.enabled` est vrai) ; générer le secret :
`kubectl create secret tls cloison-edge-tls --cert=tls.crt --key=tls.key -n cloison`.

### 5.1 Certificats — renouvellement automatique & surveillance (charte §12)

Objectif : **aucune panne TLS par expiration silencieuse**.

- **Renouvellement auto** : Caddy renouvelle par défaut ~30 jours avant
  expiration ; ne jamais poser de certificat manuel qui expirerait en
  silence. Rien qui dépende d'un cron fragile.
- **Redondance ACME** : configurer un **émetteur de secours** (ZeroSSL en
  plus de Let's Encrypt) pour qu'une panne d'une autorité ne bloque pas le
  renouvellement :
  ```caddyfile
  {
      acme_ca https://acme-v02.api.letsencrypt.org/directory
      acme_ca_root /etc/ssl/certs/ca-certificates.crt
      # Émetteur de secours (ZeroSSL) :
      acme_ca https://acme.zerossl.com/v2/DV90
      email ops@wonkom.ai
  }
  ```
- **Persistance** : le dossier de certificats/état ACME de Caddy
  (`/var/lib/caddy/`) doit vivre sur un **volume persistant** — un
  redéploiement qui perd l'état redemande des certificats et peut heurter
  les quotas Let's Encrypt.
- **Rodage** : valider d'abord contre l'**endpoint de staging** Let's Encrypt
  (`acme_ca https://acme-staging-v02.api.letsencrypt.org/directory`) pour ne
  pas épuiser les quotas de production pendant les tests.
- **Surveillance active de l'expiration** : sonde Prometheus
  blackbox-exporter `probe_ssl_earliest_cert_expiry` (ou équivalent) avec
  **alerte à J-14** ; un renouvellement qui échoue doit déclencher une alerte
  **avant** l'incident, pas après.
- **Agrafage OCSP** activé (défaut Caddy) ; en dev, HSTS optionnel (à
  réserver au vrai domaine de prod).
- **Impératif reverse-proxy** : ne journaliser **ni** l'en-tête
  `Authorization` **ni** les query strings (leçon access-log — invariant I1).

## 6. Healthchecks

| Composant | Sonde | Port | Où |
|---|---|---|---|
| edge | `tcpSocket` (pas de `/healthz` dans le proxy ; `/v1/models` exige la clé composite → 401 inutilisable en sonde) | 8787 | sondes K8s ; pas de healthcheck Docker (image distroless sans shell/curl) |
| control | `httpGet /healthz` | 8788 | sondes K8s httpGet |
| detect | `httpGet /healthz` | 8080 | healthcheck Docker (`python -c urllib…`) + sondes K8s |

## 7. Sauvegarde du registre

- Le registre nominal est le **ledger append-only** de control
  (`CLOISON_LEDGER_FILE=/data/ledger.jsonl`, volume `control-data`).
- Sauvegarde : copier `ledger.jsonl` + la clé publique de contrôle
  (nécessaire à `cloison-verify`). Le fichier est append-only : un simple
  `cp`/`rsync` suffit pour un point de restauration ; l'intégrité est
  vérifiable par `cloison-verify::verify_chain`.
- Exemple (volume compose) :
  `docker run --rm -v cloison-dev_control-data:/data -v $PWD:/backup alpine cp /data/ledger.jsonl /backup/ledger-$(date +%F).jsonl`
- En K8s : snapshot du PVC ou copie via un pod de maintenance.

## 8. Upgrade & rollback

- **Compose** : `docker compose -f deploy/docker-compose.dev.yml up -d
  --build` (reconstruction) ; rollback = `git checkout <rev>` + `up -d
  --build` ou `docker compose down && docker compose up -d` avec le tag
  précédent.
- **Helm** : `helm upgrade` puis `helm rollback cloison <revision>` ; les
  images sont taggées par SHA (`ghcr.io/coucagog/cloison-<role>:<sha>`) —
  pinner le tag dans `values.yaml` avant upgrade.
- **Rotation du sel de session** (`CLOISON_SESSION_SALT_HEX`) : change les
  jetons à chaque boot ; en production, fixer une valeur stable et la
  **faire tourner** à l'upgrade pour invalider les jetons précédents
  (invariant I7).

## 9. E2E anti-pass-through + LLM réel

Le script prouve le **masquage amont** : un proxy pass-through ÉCHOUE le test.

```bash
deploy/e2e_reel.sh                  # mode mock — aucun secret requis
# => lance un FAUX LLM local (deploy/mock_llm.py : echo + journal du corps
#    reçu) dans le réseau docker, puis ASSERTE que le corps reçu par le faux
#    LLM contient des sentinelles ⟦…⟧ et PAS la PII en clair, et que la
#    réponse finale au client contient la PII restaurée (retour 0/1/2).

CLOISON_E2E_MODE=real OPENROUTER_API_KEY=sk-or-v1-... deploy/e2e_reel.sh
# => contre le LLM réel (OpenRouter) : restauration réelle + aucun jeton
#    résiduel (téléphone comparé en chiffres normalisés). Mode `both` :
#    mock puis réel (utilisé en CI).
```

En CI : job `e2e-llm` (push sur main, `environment: e2e`, secrets
`CLOISON_ACCESS_TOKEN` / `OPENROUTER_API_KEY`, `CLOISON_E2E_MODE=both`).

## 10. SBOM & scans hors CI

```bash
docker build -f deploy/Dockerfile.proxy  -t ghcr.io/coucagog/cloison-proxy:edge    .
docker build -f deploy/Dockerfile.control -t ghcr.io/coucagog/cloison-control:latest .
docker build -f deploy/Dockerfile.detect -t ghcr.io/coucagog/cloison-detect:latest .
docker build -f deploy/Dockerfile.journal -t ghcr.io/coucagog/cloison-journal:latest .
deploy/sbom.sh     # syft (SPDX) + grype + trivy ; échec >= medium/HIGH
```

## 11. Wiring edge → contrôle (C) — activation & provisionnement

Le wiring C branche le chaînon manquant **audit → transparence** : l'edge
résout les jetons par hash (`POST /v1/control/verify`), long-polle les versions
(`GET /v1/control/version` — purge du cache à chaque rotation) et **ingère
automatiquement** ses reçus d'audit (`POST /v1/control/ingest` → journal de
transparence public). Il est **optionnel** (absent = mode N0 : auth locale
statique, pas d'ingest).

> ⚠️ **Ordre impératif** : provisionner le contrôle AVANT d'activer
> `CLOISON_CONTROL_URL` sur edge — sinon l'auth fail-closed renvoie 401
> (invariant I8 : échouer bruyamment).

```bash
# 1. Stack de base (edge + control + detect + postgres)
docker compose --profile db --env-file .env \
  -f deploy/docker-compose.dev.yml up -d --build

# 2. Provisionner le tenant + le HASH du jeton edge dans le contrôle.
#    Le clair est lu sur STDIN, haché en mémoire, jamais persisté ni affiché
#    (le stockage du contrôle ne contient que des hash — charte §9.2).
#    Le tenant_id DOIT correspondre à CLOISON_TENANT_ID du .env.
printf '%s' "$CLOISON_EXPECTED_ACCESS_TOKEN" \
  | ./deploy/provision_control.sh default

# 3. Activer le wiring dans le .env :
#    CLOISON_CONTROL_URL=http://control:8788
#    CLOISON_TENANT_ID=default          # identique au tenant provisionné
#    CLOISON_CONTROL_INGEST_INTERVAL_SECS=60
#    CLOISON_CONTROL_POLL_INTERVAL_SECS=30
#    CLOISON_CONTROL_VERIFY_CACHE_TTL_SECS=300

# 4. Redémarrer edge (seul) — control/detect/postgres inchangés.
docker compose --profile db --env-file .env \
  -f deploy/docker-compose.dev.yml up -d edge

# 5. Vérifications
curl -s -o /dev/null -w '%{http_code}\n' \
  -H "Authorization: Bearer mn_${CLOISON_EXPECTED_ACCESS_TOKEN}.${OPENROUTER_API_KEY}" \
  https://api.wonkom.ai/v1/models                      # 200 (auth via contrôle)
curl -s -o /dev/null -w '%{http_code}\n' \
  -H "Authorization: Bearer mn_jeton-inconnu.sk-x" \
  https://api.wonkom.ai/v1/models                      # 401 (fail-closed)
# Ingest visible sur la transparence (compteurs k-anonymes, jamais de texte) :
curl -s https://journal.wonkom.ai/ledger.jsonl | tail -2
```

Rotation d'un jeton (le client garde l'ancien pendant la grâce) :
`POST /admin/tenants/{id}/rotate` — la montée de `tokens_version` purge le
cache edge en ≤ `CLOISON_CONTROL_POLL_INTERVAL_SECS` ; après la grâce, le
nouveau jeton doit être propagé aux clients (ré-émission).

## 12. Tests d'intégration PostgresStore (base réelle)

Les tests `#[ignore]` de `crates/cloison-control/tests/postgres_store.rs`
(2/2) nécessitent une base réelle. Depuis l'hôte du VPS (postgres sur le
réseau interne `internal: true` — injoignable depuis l'hôte), passer par un
conteneur Rust rattaché aux réseaux du compose :

```bash
# Conteneur Rust persistant (réseau par défaut = egress OK pour crates.io)
docker run -d --name rustdev \
  -v /home/debian/Cloison/cloison:/src \
  rust:1.97-bookworm sleep infinity
docker network connect cloison-dev_cloison-internal rustdev   # DNS `postgres`

docker exec rustdev bash -lc 'cd /src && \
  CLOISON_DATABASE_URL="postgres://cloison:$CLOISON_PG_PASSWORD@postgres:5432/cloison" \
  cargo test -p cloison-control --features pg --test postgres_store -- --ignored'

# Nettoyage
docker rm -f rustdev
```

La migration `001_init.sql` est appliquée par `PostgresStore::connect` au boot
(et re-vérifiée ici) : les 4 tables (`tenants`, `api_tokens`, `policies`,
`licenses`) doivent exister — `docker exec cloison-dev-postgres-1 psql -U
cloison -d cloison -c '\dt'`. La CI vérifie que la feature `pg` **compile**
(job `test-rust`).
