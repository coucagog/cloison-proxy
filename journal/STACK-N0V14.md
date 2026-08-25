# CLOISON — STACK-N0V14 : Documentation publique docs.wonkom.ai (dette produit 📋)

> Journal de développement — écrit au fil de l'eau. Gabarit : charte §13.
> Session du 29/08/2026. Demande pilote (28/08/2026) : « Il nous faudra une
> documentation complète sur le site avec un slug spécifique. » — slug ACTÉ :
> **`docs.wonkom.ai`**. Exécution de `journal/STACK-N0V13.md` §12.
> Références : charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§11 honnêteté,
> §12 reproductibilité), handoffs `REPRISE*.md`, design system
> `deploy/journal-html/index.html` (référence, STACK-N0 §0).

## Objectif

**Une URL publique documentée qui explique le produit et permet d'installer
N0/mobile en ≤ 10 minutes** — promesse vérifiable (journal + open-core),
conforme charte §11/§12 : zéro secret, zéro PII, zéro log d'Authorization.

## Décisions

1. **Site statique multi-pages** dans le monorepo (`deploy/docs-site/`, source
   de vérité) — 8 pages : accueil, produit (architecture/promesse), install N0,
   mobile Android, API + clé composite + CLI, journal, open-core, FAQ + limites.
   **Design system du journal** (thème clair/sombre auto + bascule manuelle sans
   stockage, tokens identiques) — cohérence visuelle avec journal.wonkom.ai.
   Zéro dépendance externe (aucun CDN, aucun framework) : le site se lit sans JS.
2. **Service par Caddy `file_server`** (pas de nouveau conteneur) : surface
   d'attaque minimale (THREAT-MODEL §3.1), zéro log d'accès (défaut Caddy,
   invariant I1), TLS ACME identique à api/journal (LE + ZeroSSL).
   Le Caddyfile du repo est la source de vérité → copié sur l'hôte.
3. **Déploiement scripté et réexécutable** : `deploy/deploy-docs.sh` (copie
   contenu → `/var/www/docs.wonkom.ai`, copie Caddyfile, `caddy validate`,
   `systemctl reload caddy`, vérif HTTP). Charte §12.
4. **Contenu 100 % public** : aucun secret, aucune PII réelle, aucun détail
   opérationnel interne (IPs internes, tokens, .env). Les chiffres cités sont
   ceux des docs publiques (GO benchmark, ledger seq 12, multi-tenant).

## Ce qui a été construit

- `deploy/docs-site/index.html` — accueil : promesse vérifiable, 3 niveaux
  (N0/N1/N3), 3 preuves (journal/code/rapport), 2 chemins de démarrage.
- `deploy/docs-site/produit.html` — flux (aller/retour), moteur de détection
  (déterministe + NER afroxlmr + consensus + généralisation), niveaux,
  promesse vérifiable, limites résumées.
- `deploy/docs-site/install-n0.html` — installation ≤ 10 min (Linux/macOS/
  Windows, options, composants, config minimale, comportements garantis,
  limites honnêtes N0, vérification, mise à jour/avancé).
- `deploy/docs-site/mobile.html` — app Android v1 (WebView + WASM), build APK
  (wasm-pack + SDK), sécurité (invariants), limites v1.
- `deploy/docs-site/api.html` — 2 champs (Base URL + clé composite), curl,
  multi-tenant (X-Cloison-Tenant), endpoints, rapport k-anonyme, CLI ops
  (`cloison-cli`), variables CLOISON_*.
- `deploy/docs-site/journal.html` — accès (ledger.jsonl, control_pubkey.hex),
  construction (append-only, Ed25519, k-anonymat), vérification autonome,
  ce que le journal ne contient pas.
- `deploy/docs-site/open-core.html` — 10 dépôts publics + licences, corpus
  privé, versions publiées, procédure d'audit.
- `deploy/docs-site/faq.html` — FAQ confidentialité + limites honnêtes
  (quasi-identifiants, poste compromis, PII hallucinée, embeddings, etc.).
- `deploy/docs-site/assets/docs.css` + `assets/theme.js` — design system
  partagé (dérivé du journal), bascule de thème sans stockage.
- `deploy/Caddyfile` — bloc `docs.wonkom.ai` (root + file_server + encode gzip
  + TLS ACME double émetteur).
- `deploy/deploy-docs.sh` — déploiement idempotent.
- `README.md` — lien documentation publique.

## Comment lancer / tester

```bash
# Déploiement (hôte VPS, repo à jour) :
./deploy/deploy-docs.sh

# Vérifications :
curl -s -o /dev/null -w '%{http_code}' https://docs.wonkom.ai/           # 200
curl -s -o /dev/null -w '%{http_code}' https://docs.wonkom.ai/install-n0.html  # 200
curl -s https://docs.wonkom.ai/assets/docs.css | head -1
```

## Résultats (session terminée)

- **Commit `c0e7d65`** : 13 fichiers, 1528 insertions ; push GitHub `main`
  (bundle → VPS → fetch/merge ff → push).
- **Déploiement VPS** : `deploy-docs.sh` exécuté — contenu copié dans
  `/var/www/docs.wonkom.ai`, Caddyfile validé + rechargé, TLS ACME émis.
- **Vérification PRODUCTION (depuis le VPS)** : `https://docs.wonkom.ai/` et
  les 10 routes (8 pages + docs.css + theme.js) → **tous 200**, contenu HTML
  présent (6 occurrences « CLOISON » sur l'index).
- **Non-régression** : api.wonkom.ai → 401 sans clé (normal), journal.wonkom.ai
  → 200, ledger 13 lignes (seq 12), caddy actif, certs 87 jours.
- **⚠️ Runners GitHub** : panne toujours EN COURS (aucun run depuis le 27/08 ;
  jobs du run 25/08 échoués sans steps = jamais assignés à un runner) →
  APK Android (pas de SDK local) et binaires macOS v0.3.1 restent en attente
  de la reprise (documenté §10 STACK-N0V13 ; rien de nouveau à faire).

## Porte de sortie (dette 📋 STACK-N0V13 §12) — ✅ ATTEINTE

- [x] URL publique documentée (slug acté) : **https://docs.wonkom.ai** — 200.
- [x] Explique le produit (promesse vérifiable : journal + open-core + rapport).
- [x] Permet d'installer N0 en ≤ 10 min (guide complet, scripts publics).
- [x] Guide mobile Android (build APK documenté).
- [x] API + clé composite, CLI ops, journal, open-core, FAQ + limites honnêtes.
- [x] Conforme charte : zéro secret, zéro PII, zéro log d'Authorization
      (statique pur servi par Caddy, aucun log d'accès configuré).

## Invariants de sécurité vérifiés

- **Zéro secret** : le contenu public ne contient ni token, ni clé, ni IP
  interne, ni détail de `.env` (grep secrets sur les 8 pages : 0).
- **Zéro PII** : exemples synthétiques (Aminata Diop, user@example.com) —
  identiques aux docs déjà publiées ; aucune donnée client.
- **I1 / §12** : pas de log d'accès (Caddy file_server, aucun log configuré) ;
  le site ne reçoit ni Authorization ni query strings sensibles.
- **Exposition minimale** : zéro conteneur supplémentaire, zéro port ouvert —
  Caddy sert des fichiers statiques sur 443 (déjà exposé).

## Questions ouvertes / dette

- **Contenu vivant** : le site devra suivre les évolutions produit (versions
  N0, iOS, chiffres du journal). La mécanique est simple : éditer
  `deploy/docs-site/`, commit, `./deploy/deploy-docs.sh`.
- **Runners GitHub (APK + macOS v0.3.1)** : en attente de la reprise —
  procédure prête (CI release-n0, build APK documenté
  `mobile/android/README.md`).
- Le site docs est en français uniquement (public cible actuel) — une version
  anglaise est une piste si le pilote le demande.

## Prochaine étape

À la reprise des runners GitHub : **APK Android** (wasm-pack + SDK via CI ou
build local documenté) puis **binaires macOS v0.3.1** (remplacer les copies
v0.3.0). Dettes transverses inchangées (GPU clos — décision pilote ; calibration
prod à l'arrivée d'un client réel).
