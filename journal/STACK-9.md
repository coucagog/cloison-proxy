# CLOISON — STACK-9 : Déploiement wonkom.ai (préparation)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.
> Suite directe du STACK-8 (verdict GO). Session de déploiement, 20 août 2026.
>
> **MAJ adresses IP (fin août 2026, décision pilote)** : la cible DNS citée
> dans ce journal (`51.38.179.242`) est **dépassée** — depuis DEPLOY-1,
> wonkom/CLOISON vit sur **`144.217.81.251`**, et `51.38.179.242` est
> désormais le serveur de Mania (`vps-6dcf6a6b`). Voir
> `journal/INTEGRATION-MANIA-SN.md` §0.

## Objectif

Déployer CLOISON sur le serveur de dev wonkom.ai : swap de protection mémoire,
stack docker (edge + control + detect + postgres), TLS via Caddy, et vérifier
le produit de bout en bout sur l'hôte.

## Périmètre

**Dans :** swap 12 Go ; `deploy/` production-ready (compose, Dockerfiles,
.env.example, Caddyfile) ; secrets `.env` (jamais commités) ; build des images ;
Caddy (bloc `api.wonkom.ai` seulement — THREAT-MODEL §3.1) ; vérification
health + e2e mock ; journal + push GitHub au fil de l'eau.

**Hors :** création des enregistrements DNS (action opérateur — zone wonkom.ai
non résolvable publiquement) ; exposition de cp/detect (interdite par le
THREAT-MODEL) ; surface publique `journal.wonkom.ai` (dette : pas encore de
route lecture-seule dédiée) ; hardening N2/N3.

## Décisions

1. **Swap 12 Go** : /swapfile (fichier), persistant via `/etc/fstab`,
   `vm.swappiness=10` (`/etc/sysctl.d/99-cloison.conf`). Justification :
   l'hôte n'avait aucun swap et le VPS s'était arrêté seul (STACK-8, cause
   hyperviseur — pas OOM invité, mais la marge mémoire coûte peu).
2. **Image detect COMPLÈTE par défaut** (`CLOISON_LITE=0`, `SPACY_MODEL=
   fr_core_news_md`) : le fossé GO (grille v1.1) repose sur GLiNER + afroxlmr —
   l'image légère n'aurait pas la capacité vendue. `CLOISON_PRELOAD=all` +
   `HF_HOME=/models` (volume) : modèles pré-chargés au boot.
3. **PostgresStore actif** : compose profil `db` + `CLOISON_DATABASE_URL`
   (réseau interne) ; l'image control est construite avec la feature `pg`
   (ARG FEATURES). Le registre nominal reste le ledger embarqué.
4. **Exposition minimale** : SEUL `api.wonkom.ai` (edge) sort via Caddy.
   cp/detect/postgres restent sur le réseau interne (THREAT-MODEL §3.1) ;
   admin accessible par tunnel SSH. `journal.wonkom.ai` en attente d'une
   surface lecture-seule (dette).
5. **Mode masquage actif** (`CLOISON_AUDIT_MODE=0` dans `.env`) : le produit
   promis (pseudonymisation) ; le mode audit (observe-only) reste un
   interrupteur (`=1`), reçus persistés (`CLOISON_AUDIT_LEDGER_FILE`).

## Ce qui a été construit / configuré

- **Swap** : 12 Go actif et persistant (vérifié `swapon --show`, fstab).
- **`deploy/docker-compose.dev.yml`** : detect complet (LITE=0, spacy md,
  preload all, HF_HOME), persistance reçus audit (edge), control +
  DATABASE_URL + depends_on postgres (conditionnel), postgres healthcheck.
- **`deploy/Dockerfile.control`** : ARG FEATURES=pg (PostgresStore dans
  l'image).
- **`deploy/.env.example`** : nouvelles variables documentées
  (AUDIT_LEDGER_FILE, DATABASE_URL, LITE, SPACY_MODEL, PRELOAD, HF_*).
- **`deploy/Caddyfile`** (repo) + **`/opt/dsh/Caddyfile`** (hôte, sauvegarde
  faite) : bloc `api.wonkom.ai` → 127.0.0.1:8787 ; dsh.wonkom.ai préservé.
- **`.env`** (hôte, 0600, jamais commité) : jeton mn_, clé locataire, sel de
  session, mot de passe PG, DATABASE_URL, OpenRouter (récupérée du conteneur
  dsh — jamais affichée).

## Résultats

- Swap 12 Go : ✅ actif (0 utilisé, fstab + swappiness 10).
- Compose config : ✅ valide (profil db).
- Caddy : ✅ rechargé, ACME en cours pour api.wonkom.ai (échouera tant que le
  DNS A n'existe pas — réessaie automatiquement).
- Build images + démarrage : _en cours au moment de l'écriture_
  (Rust/sqlx + detect complet — 20-40 min).

## Invariants de sécurité vérifiés

- Zéro secret commité : `.env` en 0600, `.gitignore` couvre `.env`/`.env.*` ;
  la clé OpenRouter n'a jamais été affichée.
- Exposition minimale : seule la route edge sort (THREAT-MODEL §3.1).
- Caddy : pas de log d'Authorization ni de query strings (défaut Caddy).

## Questions ouvertes / dette

- **DNS** : zone wonkom.ai non résolvable publiquement (même la racine) —
  action opérateur : enregistrements A `api.wonkom.ai` (+ éventuellement
  dsh.wonkom.ai s'il était servi par certif) → 51.38.179.242. Sans DNS, ACME
  échoue (Caddy réessaie).
- `journal.wonkom.ai` public : surface lecture-seule à construire (ledger
  public) avant exposition.
- Wiring edge→detect (`CLOISON_DETECT_URL`) : toujours non lu par le binaire —
  aujourd'hui edge détecte avec ses détecteurs embarqués (regex/gazetteers/
  Luhn) ; le sidecar détect n'est PAS consommé par le proxy en l'état. C'est
  LA limite fonctionnelle du déploiement actuel à documenter à MLS.
- `CLOISON_ROLE` : dispatch non implémenté (deux images/binaires distincts
  aujourd'hui — le compose utilise le bon binaire par service, sans rôle).

## Porte de sortie

- [ ] Stack démarrée (edge 8787, control 8788 interne, detect 8080 interne,
      postgres interne).
- [ ] Healthchecks verts + e2e mock contre le edge déployé.
- [ ] Caddy api.wonkom.ai prêt (bloc posé ; TLS dès que DNS existe).
- [ ] Journal + push GitHub.

## Prochaine étape

Vérification de bout en bout sur l'hôte (health + e2e mock), puis : création
DNS par l'opérateur, TLS api.wonkom.ai, décision d'exposition du mode audit
(rapport de conformité) et construction de la surface journal public.
