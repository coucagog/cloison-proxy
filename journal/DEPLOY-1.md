# CLOISON — DEPLOY-1 : Déploiement sur le nouveau VPS (144.217.81.251)

> Journal de déploiement — écrit au fil de l'eau. Suite opérationnelle de
> `journal/STACK-9.md` (préparation wonkom.ai). Série `DEPLOY-N` : un journal
> par campagne de déploiement ; ce fichier couvre l'installation sur le
> nouveau VPS vierge.
>
> **MAJ adresses IP (fin août 2026, décision pilote)** : **wonkom/CLOISON =
> `144.217.81.251`** (ce serveur) · **Mania = `51.38.179.242`**
> (`vps-6dcf6a6b`, Debian 13). L'ancienne IP wonkom citée ci-dessous
> (`51.38.179.242`) est désormais le serveur de Mania. Voir
> `journal/INTEGRATION-MANIA-SN.md` §0.

## Objectif

Remplacer l'ancien serveur de dev (wonkom.ai, IP 51.38.179.242) par le
**nouveau VPS 144.217.81.251**, fraîchement réinstallé (vierge), et y
déployer la stack CLOISON complète : edge + control + detect (+ postgres,
profil `db`), avec protection mémoire anti-crash.

## Contexte — le nouveau VPS

| Ressource | Valeur | Constat |
|---|---|---|
| OS | Debian 13 (trixie), kernel 6.12 cloud | fraîchement réinstallé |
| CPU | 4 vCPU Intel Haswell (AVX2/FMA) | KVM |
| RAM | 7,6 Go | ⚠️ moins que l'ancien (11 Go) |
| Swap | 0 au départ | créé (voir décisions) |
| Disque | 75 Go (70 Go libres) | large |
| Docker | absent | installé |
| git | absent | installé |
| sudo | `debian` NOPASSWD | ✅ |
| Réseau | HF / OpenRouter / GitHub joignables | ✅ |

L'ancien hôte (6 vCPU, 11 Go) s'était déjà arrêté seul une fois (STACK-8,
cause hyperviseur ; marge mémoire faible). Le nouveau est plus petit en
RAM → **la protection mémoire est un objectif de premier ordre de cette
campagne.**

## Décisions

1. **Swap 12 Go persistant** (`/swapfile`, fstab, `vm.swappiness=10`) :
   imposé par la RAM de 7,6 Go face à un pic estimé de la stack à ~6 Go
   (detect complet au boot) et un pic du build Rust (4 rustc parallèles).
2. **Docker officiel** (script `get.docker.com`) : Docker 29.7.2 +
   Compose v5.5.0. Docker.io Debian ne garantit pas le plugin compose.
3. **Dépôt** cloné sur l'hôte dans `/home/debian/Cloison/cloison`
   (source de vérité de déploiement, comme sur l'ancien hôte) ; token
   GitHub dans `~/.git-credentials` (0600), jamais dans le repo.
4. **`.env`** (0600, secrets frais générés au déploiement, jamais committés) :
   masquage actif (`CLOISON_AUDIT_MODE=0`, décision STACK-9), detect
   COMPLET (`CLOISON_LITE=0`, `SPACY_MODEL=fr_core_news_md`), PostgresStore
   actif (`CLOISON_DATABASE_URL` + profil `db`), tenant key + jeton mn_ +
   sel de session + mot de passe PG générés par `openssl rand`.
5. **Compose avec `--env-file` explicite** : piège rencontré — compose v5
   n'a pas interpolé le `.env` de la racine du dépôt (cherché ailleurs) ;
   `docker compose --profile db --env-file <chemin> -f deploy/docker-compose.dev.yml`
   est la commande de référence sur cet hôte.
6. **Surveillance anti-crash** : script `memwatch.sh` (nohup, hôte) —
   journalise RAM/swap/top-processus + événements OOM (`dmesg`) toutes les
   10 s dans `/home/debian/Cloison/memwatch.log`. Vérifié sans événement
   OOM pendant le build.

## Actions réalisées

| # | Action | Résultat |
|---|---|---|
| 1 | Connexion SSH (clé `id_rsa_pii` + passphrase, user `debian`) | ✅ |
| 2 | `apt` : git, ca-certificates, curl, gnupg | ✅ |
| 3 | Docker + compose (get.docker.com) | ✅ 29.7.2 / v5.5.0 |
| 4 | Swap 12 Go (fstab + swappiness 10) | ✅ actif |
| 5 | Clone `coucagog/cloison` (commit `d84c395`) | ✅ |
| 6 | `.env` généré (0600) | ✅ |
| 7 | Compose `config` validé (edge/control/detect/postgres) | ✅ |
| 8 | `up -d --build` (profil `db`) | ✅ terminé |
| 9 | `memwatch.sh` actif | ✅ (aucun OOM) |
| 10 | E2E mock anti-pass-through (`deploy/e2e_reel.sh`) | ✅ **10/10 PASS** |
| 11 | Relance de la stack complète (le script e2e fait `down`) | ✅ 4 conteneurs up |
| 12 | E2E RÉEL (OpenRouter, `openai/gpt-4o-mini`) | ✅ **5/5 PASS** |
| 13 | DNS opérateur : A api/wonkom.ai → 144.217.81.251 | ✅ fait (aussi wonkom.ai, dsh) |
| 14 | Caddy 2.6.2 installé + `deploy/Caddyfile` (charte §12) | ✅ |
| 15 | Certificat Let's Encrypt `api.wonkom.ai` émis (TLS-ALPN) | ✅ auto-renouvelé |
| 16 | HTTPS vérifié `https://api.wonkom.ai` (401 sans auth) | ✅ |

## Campagne A — finitions (charte §12 / surveillance)

| # | Action | Résultat |
|---|---|---|
| A.1 | **Sonde d'expiration du certificat J-14** : `deploy/cert-expiry-check.sh` + timer systemd quotidien (`cert-expiry.timer`, `Persistent=true`) — alerte (sortie 1 + journald) si < 14 j | ✅ testé : 89 j restants, exit 0 |
| A.2 | **memwatch pérennisé** : service systemd `memwatch.service` (Restart=always), script en `/usr/local/bin/memwatch.sh` | ✅ actif |
| A.3 | Constat mémoire : le process detect (`python -m src.main`) occupe ~744 Mo RSS **au repos** (coût de l'import torch au démarrage) — **stable** (aucune fuite, 7 h), conteneur healthy, marge large | ✅ |

## Décisions restantes (attente MLS)

- **Vision** : lecture de `Doc_REF/SERVER-SPECS.png` impossible — aucun modèle
  vision déclaré dans le harness (seul `deepseek-official` / `deepseek-v4-flash`) ;
  attendre l'identifiant du modèle vision à câbler (sous-agent dédié).
- **`dsh.wonkom.ai`** : DNS → ce VPS, rien ne le sert (harness dsh sur l'ancien
  hôte) — déployer ici ou retirer le DNS ?
- **Mode audit** : exposer le rapport de conformité (k-anonyme) publiquement
  ou rester interne ?

## Résultats — vérification de bout en bout

- **Build** : 3 images construites sans erreur (proxy 48 Mo distroless, control
  avec feature `pg`, detect complet). Durée totale ~40 min sur 4 vCPU.
- **Mémoire pendant le build** : pic observé ~1,6 Go RSS, swap intact → aucun
  risque OOM (le swap 12 Go reste une ceinture de sécurité, jamais sollicitée).
- **Conteneurs** : `edge` (8787 publié), `control` (8788 interne), `detect`
  (8080/50051 internes), `postgres` (interne) — tous `Up`, detect + postgres
  `healthy`.
- **Healthchecks** : control `/healthz` → 200 ; edge `/v1/models` sans auth →
  401 (auth composite active) ; detect `/healthz` → 200.
- **E2E mock** : SUCCÈS — le faux LLM reçoit des sentinelles ⟦ et JAMAIS la PII
  en clair ; le client reçoit la PII restaurée ; aucun jeton résiduel. Un proxy
  pass-through aurait échoué. **Le produit est fonctionnel sur le nouveau VPS.**
- **E2E RÉEL** (OpenRouter, `openai/gpt-4o-mini`, clé du fichier SERVEUR) :
  SUCCÈS 5/5 — nom/téléphone/email restaurés, aucun jeton résiduel, réponse
  OpenAI valide. **Le produit fonctionne contre un vrai LLM depuis le VPS.**
- **TLS (Caddy 2.6.2, charte §12)** : certificat Let's Encrypt émis pour
  `api.wonkom.ai` (challenge TLS-ALPN-01, sans état staging — DNS déjà
  résolu par l'opérateur, émission directe production, quota LE non engagé
  outre mesure), validité 90 j, **renouvellement automatique Caddy** (~30 j
  avant expiration), état ACME persisté (`/var/lib/caddy`), émetteur de
  secours ZeroSSL configuré, **aucun log d'accès** (invariant I1 : pas de
  log d'Authorization ni de query strings), agrafage OCSP (défaut).
  Vérifié : `https://api.wonkom.ai/v1/models` → 401 sans auth (auth composite
  active derrière TLS).
- **Anti-crash** : `memwatch.log` — **0 événement** `out of memory` /
  `Killed process` pendant build + démarrage + e2e mock + e2e réel + Caddy.
  RAM disponible en permanence ~6,2-6,4 Go ; swap quasi intact (2,5 Mo/12 Go) ;
  uptime continu, aucun redémarrage.

## Constat de code (à corriger en dette)

- **`CLOISON_PRELOAD=all` n'a aucun effet au boot dans le conteneur** :
  `main.py` n'appelle `service.preload()` qu'en mode `--check`
  (`python -m src.main --check`) ; le serveur normal ne précharge rien et les
  modèles se chargent **lazily à la première requête /detect** (téléchargement
  GLiNER + afroxlmr ~3,5 Go + chargement, pic mémoire à ce moment-là). Le
  commentaire du compose (« modèles pré-chargés au boot ») est donc trompeur.
  Impact déploiement : bénéfique en mémoire au boot (rien chargé) ; à corriger
  dans le code si la latence du premier appel devient un problème, ou exposer
  `--check` via le compose.

## Risques & parades

- **OOM pendant le build Rust** (4 rustc × ~1,5-2 Go, lto thin) : swap 12 Go
  + surveillance memwatch ; le build consommait ~1,6 Go RSS au pic observé.
- **Pic au boot de detect** (`CLOISON_PRELOAD=all` : torch + GLiNER +
  afroxlmr + spaCy ≈ 4-6 Go) : survient APRÈS le build, couvert par le swap ;
  à surveiller dans le memwatch au démarrage des conteneurs.
- **Pas de crash silencieux** : tout événement `Killed process` / `out of
  memory` du `dmesg` est journalisé par memwatch.

## Porte de sortie (campagne DEPLOY-1)

- [x] Build des 3 images terminé, 4 conteneurs up (edge 8787, control 8788,
      detect 8080/50051, postgres) — profil `db`.
- [x] Aucun événement OOM pendant build + démarrage (memwatch.log vierge).
- [x] Healthchecks verts (detect `/healthz`, control `/healthz`, edge auth).
- [x] E2E mock anti-pass-through contre le edge déployé (`deploy/e2e_reel.sh`)
      — **10/10 PASS**.
- [x] E2E réel (clé OpenRouter du fichier SERVEUR) — **5/5 PASS**.
- [x] Caddy installé + bloc `api.wonkom.ai → 127.0.0.1:8787` (charte §12).
- [x] **DNS opérateur** : A `api.wonkom.ai` → **144.217.81.251** (fait —
      aussi wonkom.ai et dsh.wonkom.ai) ; TLS Let's Encrypt émis et vérifié.
- [x] Journal + push GitHub (DEPLOY-1 + `deploy/Caddyfile`).

## Dette / questions ouvertes

- Clé OpenRouter : utilisée pour l'e2e réel ; le jeton d'accès `mn_*` du
  déploiement est dans `/home/debian/Cloison/cloison/.env` (0600) — à
  communiquer à MLS pour brancher les interfaces IA.
- `dsh.wonkom.ai` pointe désormais vers ce VPS (DNS fait par l'opérateur)
  mais **rien ne le sert ici** (le harness dsh tournait sur l'ancien hôte) —
  à décider : déployer le harness dsh sur ce VPS ou retirer le DNS.
- `journal.wonkom.ai` (surface lecture-seule du ledger) : dette STACK-9
  inchangée — non exposé (THREAT-MODEL §3.1 : exposition minimale respectée).
- La stack actuelle ne consomme toujours pas le sidecar detect
  (`CLOISON_DETECT_URL` non lu par le binaire) — dette STACK-7/9 inchangée ;
  `CLOISON_PRELOAD=all` sans effet au boot (constat ci-dessus) — à corriger
  dans le code si la latence du 1er appel devient un problème.
- Surveillance : `memwatch.sh` (nohup) tourne encore sur l'hôte — l'arrêter
  une fois la campagne close, ou le transformer en service.
