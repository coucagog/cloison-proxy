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

---

## ADDENDUM — Uniformisation des coquilles docs-site (gap §3.5 du handoff racine)

> Session suivante (suite du gap « uniformité pixel-perfect des sidebars ») :
> les 9 pages de `deploy/docs-site/` avaient des coquilles divergentes
> (wrappers `docs-grid`/`docs-shell`/`docs`/`docs-wrap`/`docs-layout` ;
> sidebars `sidebar`/`docs-nav`/`docs-sidebar` ; TOC `toc`/`docs-toc`,
> `toc-title`/`toc-label`…) — générations du template à des moments
> différents.

### Ce qui a été fait

- **Coquille canonique = `index.html`** (la génération la plus récente) :
  header sticky (brand carré + nav 8 liens + bascule de thème), grille
  `docs-grid` 260px/1fr/220px, sidebar 4 groupes, `main.article` (eyebrow,
  h1, lede, sections), `aside.toc` (label + liste), footer 4 colonnes,
  4 scripts (thème, drawer no-op, scrollspy TOC, copy des codeblocks).
- **Normalisation des 8 autres pages** (produit, install-n0, mobile, api,
  journal, open-core, faq, glossaire) via un script one-shot non versionné :
  contenu d'article extrait **intact** (les classes de contenu des anciens
  wrappers sont conservées en classes additionnelles du `main` — ex. le
  glossaire garde `docs-article`/`article-inner` pour son CSS scopé) ;
  coquille remplacée par la canonique ; CSS de coquille canonique ajouté
  après le CSS de contenu propre à chaque page (les sélecteurs legacy morts
  restent dans les `<style>`, inoffensifs) ; TOC dérivé des h2/h3 réels
  (h3 → `toc-h3`) ; pager `doc-pager` uniforme ; `aria-current` correct sur
  chaque page.
- **Vérifications** : 9/9 pages avec exactement 1 header / 1 sidebar /
  1 main / 1 toc / 1 footer ; sidebars **identiques 9/9** (hors
  aria-current) ; ids h2/h3 **1:1 avant/après** (contenu intact) ; tous les
  hrefs de TOC résolubles ; zéro classe legacy dans le markup.
- **Déploiement** : commit `e87d2b6` → bundle git → VPS (fetch/ff) → push
  GitHub (`01f52414..e87d2b6c`) → `deploy-docs.sh` → `https://docs.wonkom.ai`
  200, re-vérifié **depuis le VPS** (coquilles 1× partout, sidebars
  identiques en live).

### Dette résiduelle

- Sélecteurs CSS legacy morts dans les `<style>` (ménage possible à la
  prochaine refonte) ; le **template docs-page d'Open Design** reste la
  source des divergences — toute future génération devra être re-normalisée
  ou le template aligné sur la coquille canonique.

### Retour pilote post-déploiement (à corriger en session suivante)

Sur `api.html`, `install-n0.html`, `open-core.html` : le TOC « Sur cette
page » ne suit pas la section en cours au défilement (la couleur active ne
suit pas), la sidebar « Découvrir » n'est pas identique aux autres pages,
ni les boutons précédent/suivant. **Recette pilote** : répliquer le
fonctionnement du TOC depuis `journal.html`, et la sidebar + le pager depuis
`mobile.html`. **Causes racines identifiées** : règles legacy retenues qui
masquent la sidebar — `install-n0` (`body.docs-menu-open .sidebar`, la classe
n'est plus jamais posée → sidebar invisible) et `open-core`
(`body.drawer-open .sidebar` + `transform: translateX(-100%)` → sidebar hors
écran) ; labels TOC collés « 01Xxx » (améliorer `headingLabel`). Correctif
recommandé : purger les règles de coquille legacy des `<style>` au lieu de
l'écrasement par le canonique. Détail complet dans le handoff racine
`NEXT-SESSION.md` §6-7.

---

## ADDENDUM 2 — Retour pilote §6 corrigé et déployé (TOC/sidebar/pager)

> Session suivante : exécution de la recette pilote (§6 du handoff racine).
> **Commit `9b2ed20` poussé sur GitHub, déployé en prod le même jour.**

### Ce qui a été fait

- **`_open_design/normalize-docs-site.mjs` enrichi (v2, outil conservé)** :
  1. `headingLabel` strippe aussi le numéro de tête **collé**
     (`^\d+[a-z]?(?=\S)` — « 01Endpoints » → « Endpoints »), pas seulement
     « 01 · » ;
  2. option **`--strip-legacy-shell`** : purge des règles de coquille legacy
     retenues (`.sidebar`/`.docs-sidebar`/`.toc`/`.docs-toc`/`.toc-title`/
     `.toc-label`/`.toc-list`/`.doc-pager`/`.pager`/`.docs-nav`/`.docs-grid`/
     `.docs-shell`/`.docs-wrap`/`.docs-layout`/`.docs`/`.layout`/
     `.menu-toggle`/`.menu-btn`/`.side-*`/`.nav-*`/`.site-header`/
     `.site-footer`/`.footer-*`/`body.docs-menu-open`/`body.drawer-open`,
     y compris dans les `@media`) — filtre CSS récursif (règles → `@media` →
     sous-règles), sans toucher aux sélecteurs de contenu (`.article …`,
     `.callout`, `.code-panel`, `.docs-table`, `.hero`…) ;
  3. **pager open-core dédoublonné** : suppression du `<div class="pager">`
     legacy (liens directs) qui coexistait avec le `nav.doc-pager` canonique ;
  4. **idempotent** : ré-extraction du contenu d'une page déjà normalisée
     (`<main class="…" id="top" data-od-id="article">`) et déduplication de la
     coquille canonique (marqueur `================= Header`) avant ré-ajout.
- **Application** : purge sur `api.html`, `install-n0.html`, `open-core.html`,
  `mobile.html` (la référence, pour la rendre canonique pure), plus
  `produit.html`/`faq.html`/`glossaire.html` (ménage §7, même chantier —
  règles mortes, zéro changement visuel attendu). `journal.html` **exemptée de
  la purge** (page validée par le pilote ; sa palette legacy `--accent`/
  `--border` n'a pas les alias `--jeton`/`--line` du canonique — purger la
  casserait) : seuls ses **labels TOC collés** ont été corrigés (« 01Ce que
  contient le journal » → « Ce que contient le journal »).
- **Mécanique de la correction** : après purge, la cascade de chaque page est
  exactement celle d'index.html (CSS de contenu de la page + coquille
  canonique) → sidebar/TOC/pager **identiques par construction** à
  `mobile.html`/`index.html`, scrollspy canonique (script commun) + couleur
  active `.toc a.active { border-inline-start-color: var(--jeton) }` qui suit
  au défilement.

### Vérifications (statiques, déterministes)

- 8/8 pages : exactement 1 header / 1 sidebar / 1 main / 1 toc / 1 footer,
  1 `<style>`, 1 marqueur de coquille canonique.
- Sidebars **byte-identiques au canonique** 9/9 (hors `aria-current`).
- Contenu des articles **byte-préservé** (diff : uniquement whitespace
  d'en-tête de `<main>` ; open-core : + suppression du pager legacy — vérifié
  au texte près, modulo whitespace).
- TOC : tous les hrefs résolubles ; **zéro label collé** restant ;
  `open-core` : 1 `nav.doc-pager`, 0 `div.pager`.
- Marqueurs legacy (`docs-menu-open`/`drawer-open`/`translateX`/`.docs-nav`/
  `.docs-toc`/`.pager`/`.layout`) : **0** dans les 8 pages (le seul
  `pointer-events: none` restant est la règle canonique `.doc-pager .dead`).
- Tokens canoniques (17 vérifiés : `--jeton`/`--jeton-soft`/`--jeton-text`/
  `--clair`/`--line`/…) présents dans les 7 pages purgées.
- **Limite honnête (charte §11)** : captures visuelles headless Edge
  indisponibles sur cette machine (pipes nommés bloqués par la sandbox
  locale — erreur mojo 0x5) ; la vérification repose sur la cascade
  déterministe + diff + structure, et sur le retour visuel pilote à re-faire
  sur `https://docs.wonkom.ai`.

### Déploiement (flux bundle → VPS → deploy-docs.sh)

- Commit `9b2ed20` (8 fichiers docs-site, +88/−1432).
- **Leçon opérationnelle** : `git bundle create` échoue sous la sandbox locale
  (« Refusing to create empty bundle » — pipe interne vers pack-objects
  bloqué, EPERM). Contournement déterministe : pack manuel
  (`rev-list --objects` → `pack-objects` via pipeline cmd, sans thin) +
  en-tête bundle v2 (`# v2 git bundle` + `<sha> refs/heads/main`) —
  `git bundle verify` OK localement (« complete history ») avant envoi.
- scp → VPS `/tmp` ; `git fetch` (bundle) + `merge --ff-only` → VPS à
  `9b2ed209` ; `git push origin main` (GitHub `4a938ed5..9b2ed209`) ;
  `bash deploy/deploy-docs.sh` → `https://docs.wonkom.ai/` **HTTP 200**.
- **Vérif production (depuis le VPS)** : 5 pages cibles → 200 ; api TOC
  « Endpoints » ; open-core 1 `doc-pager` / 0 `div.pager` ; marqueurs legacy
  **0** sur api/install-n0/open-core/mobile ; labels corrigés présents
  (journal/mobile/open-core) ; zéro label collé restant. **Non-régression** :
  `api.wonkom.ai` 401 sans clé (normal), `journal.wonkom.ai` 200, ledger
  13 lignes (seq 12), caddy actif.

### Dette résiduelle

- `journal.html` conserve sa palette + coquille legacy (`--accent`) — rendu
  validé pilote, mais la coquille canonique y est en partie inopérante
  (tokens `--jeton` absents). L'alignement complet passe par le **template
  docs-page d'Open Design** (ajouter les alias de tokens + régénérer), dette
  §7 inchangée.
- `produit/faq/glossaire` purgés (règles mortes) mais non re-testés
  visuellement — diff statique nul hors retraits CSS.
- Captures visuelles à re-faire côté humain (pilote).
