# CLOISON — DEPLOY-1 : Déploiement sur le nouveau VPS (144.217.81.251)

> Journal de déploiement — écrit au fil de l'eau. Suite opérationnelle de
> `journal/STACK-9.md` (préparation wonkom.ai). Série `DEPLOY-N` : un journal
> par campagne de déploiement ; ce fichier couvre l'installation sur le
> nouveau VPS vierge.

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
- **Anti-crash** : `memwatch.log` — **0 événement** `out of memory` /
  `Killed process` pendant build + démarrage + e2e.

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
- [ ] E2E réel (clé OpenRouter) — en attente de la clé.
- [ ] Caddy installé + bloc `api.wonkom.ai → 127.0.0.1:8787` (TLS dès DNS).
- [ ] **DNS opérateur** : A `api.wonkom.ai` → **144.217.81.251** (au lieu de
      51.38.179.242) — sinon ACME échoue.
- [ ] Journal + push GitHub (ce fichier + suite DEPLOY-N si nécessaire).

## Dette / questions ouvertes

- Clé OpenRouter : à fournir pour l'e2e réel (ou récupération depuis
  l'ancien VPS s'il est encore joignable).
- DNS : action opérateur (zone wonkom.ai), inchangée par rapport à STACK-9
  mais avec la NOUVELLE IP.
- Caddy : pas encore installé sur ce VPS vierge.
- La stack actuelle ne consomme toujours pas le sidecar detect
  (`CLOISON_DETECT_URL` non lu par le binaire) — dette STACK-7/9 inchangée.
