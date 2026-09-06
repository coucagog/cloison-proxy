# CLOISON × MANIA.SN — E2E-MANIA-TENANT : manuel utilisateur + sonde tenant jetable

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.
> Session des 02-03/09/2026. Suite de `journal/INTEGRATION-MANIA-SN.md`.
> Références : charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md`, `journal/STACK-7.md`,
> `journal/STACK-N0V13.md`, `MANIA.SN/ARCHIVES/STACK-4-chantier-pii.md` §53-57
> (sonde mania-pii historique), passations MANIA (helpers SSH/scp, règle
> « fichier script, jamais de commande inline »).
>
> ⚠️ **CONTRAINTE PILOTE** : les deux serveurs sont **en production**
> (Mania `51.38.179.242` · wonkom `144.217.81.251`). La sonde a été exécutée
> **isolée, réversible, sans modification du gabarit, sans redémarrage
> d'aucun service partagé, sans clé LLM réelle, sans donnée réelle**.

## Objectif

1. Livrer un **manuel d'utilisation HTML** (clique-pour-copier) : parcours
   **script** d'installation/configuration par OS en premier, parcours
   **manuel** en second, le tout en volets découvrants — périmètre strict =
   ce qui est prouvé.
2. Exécuter une **sonde de bout en bout** : un tenant Hermes jetable dont
   l'agent appelle le LLM **via un edge CLOISON**, masquage/restauration
   vérifiés, sur le VPS Mania en production.

## Périmètre

**Dans :** manuel v2 (`deploy/docs-site/manuel.html`) ; scripts
`deploy/configure-n0.sh` / `deploy/configure-n0.ps1` (nouveaux — **à
valider**) ; kit de sonde `SERVEUR/` (scripts + mock LLM) ; exécution
isolée ; journalisation ; dépouillement complet.

**Hors :** patch du gabarit `/opt/hermes/gabarit` (phase 1 — décision pilote
séparée) ; activation de packs (`PII=1`) ; tout changement sur les tenants
existants ; déploiement du manuel sur `docs.wonkom.ai` (acte séparé).

## Décisions

1. **Manuel v2** : `<details>/<summary>` natifs (zéro dépendance), volet A
   « Script (recommandé) » = installer (`install-n0.*`) → configurer
   (`configure-n0.*`) → démarrer (`start-n0.*`) par OS ; volet B « Manuel »
   = blocs env des deux OS + NER + deux champs ; sections suivantes en
   volets fermés. Refonte déléguée à un sous-agent (spec complète, 20 blocs
   copiables vérifiés 1:1).
2. **Scripts `configure-n0.*`** : génèrent `n0.env(.ps1)` (0600) +
   `start-n0.*`, affichent la clé composite une fois, `-Start`/`--start`
   optionnel. **Nouveaux — pas encore prouvés** (étiquette honnête dans le
   manuel).
3. **Sonde** : tenant `sonde-cloison`, agent Hermes **seul** (pas de WebUI,
   pas de labels Traefik, pas d'inscription mania-app), réseau
   `internal: true`, edge CLOISON raccordé au réseau tenant.
4. **Edge** : image GHCR non publique (cf. Découvertes ①) → **repli binaire
   release publique** (`install-n0.sh` officiel, checksums SHA-256 vérifiés,
   `--prefix /tmp/cloison-n0`), exécuté dans `debian:bookworm-slim`
   (read-only, `cap_drop ALL`).
5. **Amont** : `CLOISON_MOCK_MODE` ne répond pas lui-même (Découvertes ②) →
   **mock LLM maison** (python, écho OpenAI-compatible **avec SSE**,
   découpage en 3 chunks pour exercer le buffer-and-scan) dans un conteneur
   sur le réseau tenant.
6. **Auth** : clé composite `mn_<jeton>.<clé amont>` ; pour la sonde, la
   partie amont est la chaîne `mock` (aucune clé réelle). Le jeton vit dans
   `/opt/hermes/sonde-cloison/.env` (0600 root) — **jamais affiché** (règle
   absolue des passations), caviardage systématique dans ce journal.
7. **Masquage actif** : `CLOISON_AUDIT_MODE=0`. Piège : `=1` est
   l'**observe-only** (STACK-4) — notre sonde l'a d'abord pris pour le
   masquage (Découvertes ④).
8. **Connexion serveur** : helpers documentés `.tmp-deploy/mania-ssh.ps1` /
   `mania-scp.ps1` (clé + askpass encapsulés) ; **toujours** livrer les
   commandes en fichier script puis `sudo bash` (leçon des passations,
   re-vérifiée : `|` et quotes meurent en inline).

## Ce qui a été construit (au fil de l'eau)

- [x] `deploy/docs-site/manuel.html` v2 (volets, script d'abord, 20 blocs
      copiables) — refonte par sous-agent.
- [x] `deploy/configure-n0.sh` + `deploy/configure-n0.ps1` (nouveaux).
- [x] Kit de sonde dans `SERVEUR/` : `recon-mania.sh`,
      `sonde-cloison-e2e.sh`, `sonde-phase2..8.sh`, `sonde-cleanup.sh`,
      `mock-llm.py` (v2 SSE) — reproductibles.
- [x] Recon lecture-seule du VPS Mania (état initial consigné).
- [x] Sonde exécutée de bout en bout (voir Résultats).
- [x] Dépouillement complet — **état initial restauré et vérifié**
      (conteneurs et réseaux identiques à la recon).
- [x] Ce journal.

## Comment lancer / tester (runbook reproductible)

```bash
# 0. Connexion (depuis la machine locale) :
Set-ExecutionPolicy -Scope Process Bypass -Force
& .tmp-deploy\mania-ssh.ps1 'sudo -n true'            # vérif sudo NOPASSWD
& .tmp-deploy\mania-scp.ps1  <script-local> /tmp/<script>

# 1. Recon lecture-seule :
& .tmp-deploy\mania-scp.ps1 SERVEUR\recon-mania.sh /tmp/recon-mania.sh
& .tmp-deploy\mania-ssh.ps1 'bash /tmp/recon-mania.sh'

# 2. Sonde (détachée, log /tmp/sonde-cloison-e2e.log) :
& .tmp-deploy\mania-ssh.ps1 'nohup sudo bash /tmp/sonde-cloison-e2e.sh </dev/null >/dev/null 2>&1 &'
#    puis phases 2..8 au besoin (voir scripts SERVEUR/), suivi :
& .tmp-deploy\mania-ssh.ps1 'tail -n 30 /tmp/sonde-cloison-e2e.log'

# 3. Dépouillement (retour à l'état initial) :
& .tmp-deploy\mania-ssh.ps1 'sudo bash /tmp/sonde-cleanup.sh'
& .tmp-deploy\mania-ssh.ps1 'docker network rm sonde-cloison_sonde-net'   # résidu possible
```

## Résultats (chronologie)

### Recon (état initial — 02/09, 23:00 UTC)
- Hôte `vps-6dcf6a6b` (51.38.179.242), debian, up 11 j, 5,9 Gi RAM dispo,
  58 Go disque. Docker 29.7.2.
- Tenants : `agnes khalil oniang ridwan skd wagui` (agent+webui chacun) +
  `mania-app-1 traefik mania-documents mania-transcription eager_vaughan`.
- **Tous les réseaux tenants : `internal: false`** (confirme PII=0 partout).
- Hermes Agent **v0.20.5** (2026.8.19), config `_config_version 38`,
  `stt.provider: mania` câblé. Aucun cloison/pii sur l'hôte.
- Gabarit root-only (`sudo` requis), sans `.git` (dette gabarit vivant ≠
  dépôt, confirmée).

### Sonde (phases — voir scripts `SERVEUR/`)
- **Auth composite : VALIDÉE** — `GET /v1/models` via edge → **HTTP 200** ;
  clés invalides → **401 fail-closed** (observé, y compris des appels Hermes
  sans clé au démarrage, rejetés proprement).
- **Verrou egress : PROUVÉ** — depuis l'agent (réseau internal) :
  `curl https://1.1.1.1/` → **exit 7** (couche IP refusée).
- **Profil fournisseur : PROUVÉ** — `model.provider: custom:cloison` posé
  via `hermes config set` (6 clés), `model.base_url` retiré (#25107),
  redémarrage agent, `grep` = 1.
- **Roundtrip E2E : PROUVÉ** — `hermes -z` ×2 à travers le edge (mode N0
  vault+NER, masquage actif) :
  - **Ce que le mock (le « LLM ») a reçu** (preuve anti-pass-through) :
    `Rappel : ⟦iz3om6tealbzwfjxh4vmnb6gqq·GZA⟧ ⟦qpdbuxuna3abcafhlqw2sicimu·PE⟧, ⟦pl4vcfh33ewictcspsldiq2zdu·PH⟧, [VILLE_SN].`
    → **aucune PII en clair** : nom en 2 sentinelles (gazetteer + alias),
    téléphone `⟦·PH⟧`, ville **généralisée** `[VILLE_SN]` (jamais tokenisée).
  - **Ce que l'agent a répondu au client** (restauration) :
    `Rappel : Aminata Diop, +221 77 123 45 67, [VILLE_SN].`
    → nom et téléphone **restaurés à l'identique** ; la ville reste
    généralisée (comportement N0 attendu, irréversible par design).
  - **Streaming SSE : PROUVÉ** — Hermes tourne `stream:true` par défaut
    (contrairement aux tenants mania-pii historiques) ; le mock a découpé la
    réponse en 3 chunks et CLOISON a restauré correctement (buffer-and-scan).
- **NER embarqué côté VPS : dégradation gracieuse PROUVÉE** — inférence en
  échec sur ce bundle (cf. Découvertes ⑥) → spans ignorés avec warn, **les
  gazetteers + regex ont assuré le masquage complet** (aucune fuite).

### Dépouillement
- Tenant, conteneurs, volumes, réseau, images temporaires et scripts
  supprimés. **Vérifié : conteneurs et réseaux identiques à la recon.**

## Découvertes (bugs / écarts produits — à escalader)

1. **Image GHCR non publique** : `ghcr.io/coucagog/cloison-proxy:edge` →
   `unauthorized`. Le `deploy/docker-compose.dev.yml` la référence mais elle
   n'est pas publiée (les déploiements historiques buildaient en local). →
   publier l'image ou corriger le compose/la doc.
2. **`CLOISON_MOCK_MODE` ne répond pas lui-même** : avec `=1`, le edge
   écoute (`mock_mode=true`) mais **forwarde quand même** vers l'amont
   (défaut `http://127.0.0.1:1`) → 502. Le « mock » du produit est
   `deploy/mock_llm.py` en amont. Sémantique à documenter (CONFIG.md).
3. **Crash boot : N0 + audit** — `CLOISON_VAULT_PATH` **et**
   `CLOISON_AUDIT_MODE=1` ensemble → `failed to hash audit policy` (release
   latest). N0 + `AUDIT_MODE=0` boote. **✅ CORRIGÉ dans le code**
   (02-03/09) : `receipt::policy_hash` sérialisait la Policy via
   `serde_json::to_value` — les clés `DetectorKind::Gazetteer("ville_sn")`
   de `generalization` (politique N0) produisent une clé objet refusée
   (« key must be a string »). Fix : conversion des clés `DetectorKind` en
   string stable (`Display`) pour `generalization`/`cardinality_thresholds` ;
   hash **bit-identique** pour les politiques sans clé à données (chemin
   serveur). Tests : 19/19 cloison-audit (dont 2 régressions nouvelles),
   core 90 + 17 invariants, EXIT=0 (toolchain GNU locale). Commit local —
   publication/CI à planifier.
4. **`CLOISON_AUDIT_MODE=1` = observe-only** (compte sans masquer, STACK-4) :
   notre première interprétation était inversée ; les phases 5-6 ont tourné
   en observe-only (mock alimenté en clair = attendu dans ce mode). Le
   rapport k-anonyme signé a été exercé au passage : `publishable:true`,
   compteurs `Email:3, Gazetteer(nom_sn):6, Gazetteer(ville_sn):6, PhoneSn:6`.
5. **`CLOISON_AUDIT_K=1` est clampé à 2** (rapport `"k":2`) — à documenter
   (borne minimale de k-anonymat).
6. **NER embarqué en échec sur le VPS** : `BroadcastIterator … 512 by 5750`
   (tokenizer/modèle du bundle `latest`) → spans ignorés. La preuve locale
   Windows (STACK-N0V13) passait → investiguer la cohérence du bundle
   publié (ou la longueur d'entrée). Dégradation gracieuse conforme au
   design (jamais d'erreur, jamais de fuite).
7. **Docker + `--network none`** : impossible de `network connect` tant que
   le conteneur est attaché à `none` (« container cannot be connected to
   multiple networks with one of the networks in private (none) mode ») →
   il faut d'abord `docker network disconnect none`.
8. **Hermes envoie des appels sans clé au démarrage** (bursts de 401
   fail-closed avant l'appel réel) — non bloquant, à consigner dans la doc
   d'intégration (phase 1).
9. **Leçon de process (répétée)** : le quoting PowerShell→SSH tue les
   commandes inline (`|` dans les formats docker, quotes) → **fichier
   script, toujours** (déjà la règle des passations). Et un contrôle d'état
   doit capturer UNIQUEMENT `{{.State.Status}}` (la sortie de `docker run -d`
   polluait la capture → faux repli).

## Invariants de sécurité vérifiés

- **Zéro clé LLM réelle** (partie amont de la clé composite = `mock`),
  **zéro donnée réelle** (textes synthétiques uniquement).
- **Zéro PII en clair chez le « fournisseur »** : le mock n'a reçu que des
  sentinelles ⟦…⟧ et `[VILLE_SN]` (preuve anti-pass-through, MOCK_RECU).
- **Fail-loud** : 401 sur clés invalides (y compris appels Hermes sans
  clé) ; restauration bornée (réponse restaurée exacte, aucun jeton
  résiduel).
- **Zéro secret affiché** : jeton jamais imprimé (caviardage systématique
  `<CAVIARDE>`), `.env` 0600 root, scripts 0600.
- **Aucun tenant existant touché, aucun restart de service partagé, aucun
  label Traefik, aucune inscription mania-app** ; dépouillement vérifié
  (état initial restauré).
- **Journal sans clair ni mapping** : ce journal ne contient que des
  compteurs et des sentinelles d'exemple.

## Questions ouvertes / dette

- Patch définitif du gabarit (`nouveau-tenant.sh` : `PROXY_PII=cloison-edge`,
  clé composite dérivée, section SOUL ⟦…⟧, contrôle d'activation) — phase 1,
  décision pilote séparée, dans le dépôt d'abord.
- Escalades produits ① ② ③ ⑤ ⑥ (image GHCR, mock_mode, crash N0+audit,
  clamp k, bundle NER VPS).
- Publication des `configure-n0.*` dans le dépôt public (avec preuve
  d'exécution, comme `install-n0.*`).
- Répétition éventuelle avec **clé réelle** (OpenRouter/GLM via le magasin
  DSH) — même runbook, partie amont réelle, coût minime.
- UX clé composite pour les clients (à trancher en phase 1).

## Porte de sortie

- [x] Manuel livré dans le dépôt (volets, script d'abord) — non déployé.
- [x] Sonde exécutée de bout en bout avec preuves (sentinelles amont,
      restauration client, egress 7, 401 fail-closed, SSE).
- [x] État initial du serveur restauré et vérifié.
- [x] Journal complété (ce fichier).

## Session 03/09/2026 — Règlement des dettes (a)(b)(c)(d) + question N0

> Suite du journal. Décision pilote : régler les dettes une bonne fois.
> Rappel de niveau : la sonde a fait tourner la **configuration** N0 du moteur
> (coffre + NER + généralisation) sur le VPS Mania = **niveau N3 hébergé**
> (l'opérateur pourrait lire). N1/N0 exigent le moteur chez le client.

### (a) Dettes produits — RÉGLÉES côté code + doc

- **Crash N0+audit CORRIGÉ** (`failed to hash audit policy`) : cause racine =
  `serde_json::to_value(&Policy)` refusait les clés `DetectorKind::Gazetteer(…)`
  (variante à données → clé objet non-string) présentes dans `generalization`
  de la politique N0. Fix `cloison-audit/src/receipt.rs` : conversion des clés
  `DetectorKind` en string stable (`Display`) pour `generalization` /
  `cardinality_thresholds` ; hash **bit-identique** pour les politiques sans
  clé à données (chemin serveur inchangé). **Testé localement** (toolchain GNU
  portable) : cloison-audit 19/19 dont 2 régressions nouvelles, cloison-core
  90 + 17 invariants, EXIT=0. Commit local ; publication CI/VPS à planifier.
- **Docs corrigées** : `docs/CONFIG.md` (MOCK_MODE ne répond pas lui-même —
  amont factice 127.0.0.1:1 ; AUDIT_MODE `0`=masquage actif / `1`=observe-only
  sans masquage ; AUDIT_K plancher 2) ; `deploy/docker-compose.dev.yml`
  (avertissement image GHCR non publiée → build local ou release).
- **NER embarqué sur Linux** : échec d'inférence re-confirmé en sonde réelle
  (`BroadcastIterator 512 by 5755` — bundle `latest` incohérent ; la preuve
  Windows locale passait). Dégradation gracieuse conforme (spans ignorés,
  gazetteers+regex ont couvert). **Reste à escalader** (republier un bundle
  cohérent).

### (b) Phase 1 — gabarit v4 DÉPLOYÉ

- `nouveau-tenant.sh` v4 (dépôt MANIA + `/opt/hermes/gabarit`, backups
  horodatés ×2) : `PROXY_PII="edge"` (conteneur `${SLUG}-edge` PAR tenant),
  service edge dans le compose (image locale `mania-cloison-edge`,
  `CLOISON_AUDIT_MODE=0`, coffre N0 + NER, jeton `mn_` + clé locataire +
  passphrase coffre générés dans le `.env` 0600, jamais affichés), profil
  `providers.cloison` (base URL interne sans secret, `key_env=
  OPENROUTER_API_KEY` — le client saisit la clé composite dans sa WebUI),
  section SOUL réécrite pour les **sentinelles ⟦…⟧** (+ `[VILLE_SN]`), checks
  adaptés (probe 404 = auth acceptée), pré-vol sur l'**image** (fail-closed).
- **Correction critique du même jour** : l'edge sur le seul réseau internal ne
  peut pas joindre l'amont réel (502 constaté en sonde) → le service edge est
  sur **deux réseaux** (`$SLUG-net` internal + `bridge` egress) ; l'agent
  reste sur l'internal seul (barrière egress intacte).
- Image `mania-cloison-edge:latest` **construite** via
  `ops/build-cloison-edge.sh` (release publique + debian:bookworm-slim) —
  correction d'un bug du script (cp même-fichier sous `set -e`).
- Packs : **toujours PII=0** (activation = décision pilote par verticale).

### (c) Sonde à clé RÉELLE (OpenRouter) — PROUVÉE avec limite honnête

- Tenant jetable + edge (image locale, coffre N0, masquage actif) + clé
  composite réelle (clé OpenRouter du magasin DSH, **jamais affichée**,
  `.env` 0600, supprimée au dépouillement).
- **PROUVÉ** : `hermes -z "Bonjour…"` → réponse « OK » réelle via OpenRouter
  (deepseek/deepseek-v4-flash) — auth composite, egress 7, masquage avant
  l'amont (OpenRouter n'a reçu **aucune PII en clair**, uniquement des corps
  de sentinelles), streaming.
- **Limite rencontrée (connue, charte §16)** : le modèle **dépouille les
  crochets ⟦…⟧** dans ses réponses → la restauration échoue et le client voit
  les corps bruts (dégradation d'usage, **pas de fuite** : les corps sont des
  HMAC irréversibles sans la clé du coffre). Pistes produit déjà prévues :
  substitution **faux réaliste** par politique (irréversible, à vérifier
  avant émission) et robustesse des sentinelles (recherche ouverte).
- **Pièges de session consignés** : fichier `.env` écrit sans saut de ligne
  final → l'append `tee -a` a concaténé la clé (corruption silencieuse,
  corrigée par réécriture + `--force-recreate` de l'agent) ; egress bridge
  de l'edge ; commandes inline avec `|` interdites (×3) — fichier script,
  toujours.
- Jambe GLM (z.ai) : non exécutée (conclusion attendue identique ; à rejouer
  sur demande). Dépouillement complet, état initial vérifié.

### (d) Manuel EN LIGNE

- `manuel.html` livré dans le repo wonkom puis `deploy-docs.sh` exécuté :
  **https://docs.wonkom.ai/manuel.html → HTTP 200** (Caddy rechargé). La page
  n'est pas encore reliée depuis la sidebar du site (uniformité de coquille —
  à faire proprement).

### Reste ouvert (escalade/arbitrage)

1. ~~Publication du fix (a) via CI/VPS~~ **FAIT** (bundle → wonkom → `push origin main`,
   commits `6dc6153` + `30ccf4c` + `ce1201e`).
2. ~~Bundle NER Linux incohérent (⑥)~~ **CONTOURNÉ** : release **v0.3.1 épinglée** dans
   `build-cloison-edge.sh` (vérifié `ner_actif=1, ner_echecs=0` sur wonkom). Le bundle
   `latest` reste à corriger côté publication.
3. ~~Sentinelles vs modèles qui nettoient ⟦…⟧~~ **IMPLÉMENTÉ** : `CLOISON_REALISTIC_FAKE=1`
   — module `cloison-core/src/fake.rs` (faux déterministe par session via le corps du
   jeton, jamais la valeur réelle, irréversible ; PERSON/Gazetteer(nom_sn)/PhoneSn/Email,
   repli sentinelle pour les autres types), câblé dans `engine.process_spans` +
   `config.rs`/`handlers.rs` (opt-in), gabarit v4.1 (pass-through par tenant), docs
   CONFIG. **Tests : core 96/96 + 17 invariants, proxy check EXIT=0.** Poussé (`ce1201e`).
   ⚠️ Pas encore en release : l'image Mania (v0.3.1) ne la porte pas — voir NEXT-SESSION.
4. ~~Activation des packs~~ **FAIT** : sante, droit, finance, gouvernement, ong = `PII=1`
   (VPS + repo MANIA, backups `.bak-20260903`). Les autres packs restent `PII=0`.
5. Jambe GLM + rotation des clés (exposées en session, pilote prévenu — il s'en charge
   avant la prochaine session).

## Prochaine étape

Arbitrage pilote : (1) publier le fix (a) (bundle → GitHub via le VPS,
procédure n3-*.sh) ; (2) republier un bundle NER cohérent pour Linux ;
(3) trancher la stratégie sentinelles/faux-réaliste pour les modèles qui
nettoient ⟦…⟧ ; (4) activer les packs sensibles (`PII=1`) une fois ces
verrous levés ; (5) relier `manuel.html` à la sidebar du site docs.

---

## Session 04/09/2026 — Release CLOISON v0.3.2 (faux réaliste) PUBLIÉE + edge Mania reconstruit

> Suite du journal. Objectif = NEXT-SESSION item 1 : rendre `CLOISON_REALISTIC_FAKE`
> effectif côté tenants (l'image Mania v0.3.1 ne portait pas `ce1201e`).

### Release v0.3.2 — publiée sur coucagog/cloison-proxy

- **9 assets** : linux x86_64 + windows (mingw, nommé `-msvc` comme v0.3.1) +
  macOS (copies v0.3.0, caveat dans le corps) + bundle NER (identique v0.3.1,
  `ner_echecs=0`) + 3 libs onnxruntime + **checksums 8 entrées** (leçon v0.3.0
  appliquée). Téléchargements publics 200.
- **Portes VPS (rustdev)** : tests workspace verts + e2e_n0 **8 passed**.
  ⚠️ Les portes avaient D'ABORD échoué : `Config.realistic_fake` absent des
  initialisateurs des tests e2e du proxy (cassé par `ce1201e`, jamais vu en
  local car la porte locale était `cargo check`, pas `cargo test` — la release
  a été **arrêtée avant publication**, portes corrigées, relancée).
- **Smoke Windows réel** : exit 0 (masquage + restauration prouvés), sha256 du
  binaire == checksums.
- **Runners toujours en panne** → builds manuels sur wonkom (doctrine
  STACK-N0V13 §10).

### Image mania-cloison-edge reconstruite (VPS Mania)

- Pin `--version v0.3.2` posé dans `/opt/hermes/gabarit/ops/build-cloison-edge.sh`
  (backup `.bak-<stamp>` — le script n'avait AUCUN pin, il installait `latest`) ;
  image reconstruite, **backup tag `mania-cloison-edge:backup-20260904-222634`** ;
  **aucun conteneur tenant touché** (aucun `-edge` en service ; seuls les
  prochains provisionnements PII=1 consomment `:latest`).
- **Sonde faux réaliste PROUVÉE (parcours réel, donnée synthétique)** : edge
  v0.3.2 isolé + mock local → le « fournisseur » a reçu
  `Maimouna Yacine, +221 77 256 51 21, [VILLE_SN]` — **zéro PII réelle, zéro
  sentinelle, ville généralisée** ; le client reçoit le faux (irréversible,
  conforme au contrat). Nettoyage complet, aucun résidu, état initial vérifié.

### Corrections de fond committées (monorepo)

- `n0-release-assets.sh` : `RELEASE_ID` résolu en tête (était après les
  checksums → `unbound variable`) ; **uploads bundle+libs AVANT checksums**
  (la leçon v0.3.0 avait récidivé : 4 entrées au lieu de 8) ; téléchargement
  des assets par l'API avec jeton (`browser_download_url` = **404 sur un
  DRAFT**) ; `checksums.txt` exclu du skip et remplacé sans doublon.
- Bits exécutables `deploy/*.sh` restaurés dans git (644 par checkout Windows →
  `Permission denied` en prod).
- Hygiène : en-têtes IP des journaux (DEPLOY-1, STACK-9), journaux
  E2E-OPEN-DESIGN + MOBILE-BUILD-LOCAL versionnés, fix wasm `Cargo.toml`.

### Pièges payés (à retenir pour la prochaine release manuelle)

1. **Une porte locale `cargo check` ne couvre pas les tests** — la CI les
   couvrait, les runners en panne ne le font plus : la porte VPS doit être
   `cargo test` et son exit code doit faire foi (un pipeline interne à
   `bash -lc` masque l'échec).
2. **Draft GitHub** : les `browser_download_url` répondent 404 tant que la
   release n'est pas publiée → API asset + jeton.
3. **Checksums après les uploads de TOUT**, jamais avant ; et un `EXISTING`
   capturé avant une suppression ment ensuite (skip inopportun).
4. **Modes git perdus par Windows** : `chmod +x` dans l'index pour tout script
   exécuté en prod.
5. Le quoting PowerShell→SSH a mordu **trois fois de plus** (grep `\|`/`df|tail`,
   parenthèses ERE échappées à tort, here-strings CRLF) — fichier script LF,
   toujours, même pour un `tail`.

### Reste ouvert

- ~~Bundle NER « latest » incohérent sur Linux~~ → **RÉSOLU PAR LA MESURE
  (04/09)** : sha256 identiques 6/6 entre le bundle v0.3.1, le bundle v0.3.2
  et le volume `detect` actuel ; NER **vérifié sur l'image Mania** (`ner_actif=1,
  ner_echecs=0`, labels=9) ; et `releases/latest` == v0.3.2 (bundle cohérent
  servi). Reste : revérifier à la PROCHAINE régénération du volume (la cause
  du 512×5755 n'est pas élucidée, seulement contournée).
- macOS = copies v0.3.0 (runners en panne).
- ~~Arbitrage pilote sentinelles vs faux réaliste par tenant/verticale~~ →
  **ACTÉ le 05/09** : politique générale validée par MLS (sentinelles partout
  par défaut ; faux = opt-in par tenant sous 4 conditions mesurées ; zéro
  changement en prod — aucun tenant PII=1 en service). Détail :
  `journal/ARBITRAGE-05-SENTINELLES-VS-FAKE.md` §6.
- ~~`manuel.html` → sidebar~~ **FAIT (04/09)** : lien « Manuel d'utilisation »
  dans le groupe Déployer des 9 pages (sidebars uniformes 9/9 vérifiées) +
  lien retour « Documentation » dans le header du manuel ; déployé sur
  docs.wonkom.ai, vérifié en prod (10 pages 200, lien 9/9, non-régression).
- Jambe GLM (item 5).

---

## Session 05-06/09/2026 — Tenant MANIA réel `demo-cloison` : premier run compose v4.1 + cause du 512 élucidée + correctifs NER

> Suite du journal. Objectif pilote : créer à la main, de bout en bout, un
> tenant MANIA qui utilise CLOISON convenablement. C'était le **premier
> provisionnement réel du compose v4.1** (aucun tenant PII=1 n'existait).

### Provisionnement manuel (VPS Mania, fichiers script, jamais d'inline)

- Pré-vol vert (image edge v0.3.2 du 04/09, gabarit v4.1 = sha256 du dépôt,
  secrets 0600, 5 packs PII=1) — **1 faux négatif de ma sonde** : grep
  `NOUVEAUTE` sans accent ; l'empreinte, elle, ne mentait pas.
- **🔵 Découverte 1 — `bridge` intégré refusé par compose** : `networks:
  [$SLUG-net, bridge]` → `network-scoped aliases are only supported for
  user-defined networks`. Les sondes d'avant passaient par `docker run` +
  `docker network connect` : **le chemin compose n'avait jamais été exécuté**.
  Correctif : réseau `egress` user-defined déclaré dans le compose (aucun
  prérequis hôte). Vivant (backup horodaté) + dépôt ManIA (`nouveau-tenant.sh`).
- **🔵 Découverte 2 — garde `sh -lc 'command -v hermes'`** : le login shell
  réinitialise le PATH du conteneur → `profil NON cable` à tort. Correctif :
  test du binaire par chemin (`/opt/hermes/.venv/bin/hermes`). Vivant + dépôt.
- Câblage manuel du profil `custom:cloison` (forme sonde-real.sh) → `count=1`.
- **Activation RÉELLE prouvée** : amont DeepSeek direct
  (`CLOISON_UPSTREAM_BASE_URL=https://api.deepseek.com/v1`, modèle
  `deepseek-chat`), clé composite saisie en WebUI → `hermes -z Bonjour`
  répond. Egress fermé = tout appel passe par l'edge.

### Sonde PII réelle — deux faits mesurés

1. **« Jeton Diop »** : le modèle a reçu `⟦…·GZA⟧ Diop` et a **paraphrasé** la
   sentinelle opaque en « Jeton » (comportement modèle sur jeton opaque,
   même famille que le dépouillement deepseek-v4-flash) ; « Diop » était en
   clair → recopié tel quel. Restaurations email/tél/ville correctes.
2. **`512 by 6737` dans les logs edge** : le NER échouait sur le prompt
   système Hermes (6737 tokens). La **cause du 512×5755 (04/09, non élucidée,
   « contournée ») est enfin élucidée** : les embeddings de position du
   graphe ONNX sont figés à **512** ; `light_ner` inférait le texte entier
   sans fenêtrage. Le « contournement » du 04/09 (reconstruire le volume)
   était une coïncidence : la sonde E2E utilisait des messages courts.

### Reproduction locale contrôlée (hors prod)

- Kit local : binaire **Windows v0.3.2** (`SERVEUR/v032-win/`) = même version
  que l'edge déployé, modèle NER + dll (`_open_design/n0-e2e/ner/`), mock
  fournisseur (`mock-llm-local.py`), sonde `SERVEUR/probe-ner-local.ps1`
  (courte vs longue ~720 tokens, capture MOCK_RECU).
- **Confirmé** : courte = NER OK mais « Aminata » masqué (gazetteer) et
  **« Diop » en clair** ; longue = `inférence échouée 512 by 720` + prénom/patronyme
  non couverts par le NER. Restauration exacte des deux côtés.

### Correctifs (cette session, validés localement)

- `cloison-proxy/src/light_ner.rs` : **fenêtrage** (`windows()` : ≤511 tokens,
  chevauchement 64, couverture totale — jamais de perte de queue, contrairement
  à la troncature HF du sidecar référence) + **`[CLS]` préfixé à chaque fenêtre**
  (le sidecar encode avec les tokens spéciaux par défaut ; le portage les avait
  perdus). Déduplication des spans de chevauchement. Tests unitaires
  (`windows_*`, `dedup_*`).
- `cloison-core/src/detection.rs` : gazetteer `nom_sn` = prénoms + **66
  patronymes** (liste du bench) ; **frontière de mot** dans `Gazetteer::find`
  (les patronymes courts « Sy », « Ba », « Fall » ne matchent plus
  l'intérieur de « système », « banane », « falling »). Tests dédiés.
- `engine.rs` : test `test_restore_canonicalized_value` mis à jour (2 → 3
  restaurations : le patronyme est désormais masqué puis restauré).
- **Validation locale** (toolchain GNU android-tools, `--offline`) :
  `cargo test -p cloison-core` **98/98** · `cargo test -p cloison-proxy --lib`
  **24/24** · `cargo build --release` OK (6m44). **Sonde v2 sur le binaire
  corrigé** : `inférence échouée = 0` sur le message long ; « Aminata » et
  « Diop » masqués (2× ·GZA) ; **zéro valeur réelle reçue par le mock** ;
  restaurations exactes.

### Reste ouvert (suite serveur — avec le pilote)

- **Release v0.3.3** (builds manuels wonkom, runners en panne ; portes
  `cargo test`, leçon v0.3.2 : la porte locale `cargo check` ne couvre pas) →
  **reconstruire `mania-cloison-edge` pin v0.3.3** → re-sonde `demo-cloison`
  (la sonde PII réelle doit montrer zéro sentinelle brute ET zéro valeur en
  clair, avec le vrai modèle).
- **Commits ManIA à pousser** (egress user-defined, garde `sh -lc`, + dettes
  cosmétiques v2/v3 du README) — le gabarit vivant est déjà corrigé.
- 🔴 **`response_format` refusé par DeepSeek direct** (`400 : This
  response_format type is unavailable now` — Hermes envoie un format que
  l'API DeepSeek n'accepte pas ; l'appel aboutit quand même par retry/mode
  dégradé) : à corriger (config Hermes du tenant ou strip côté edge — à
  trancher).
- Arbitrage sentinelles/faux pour `demo-cloison` selon le comportement du
  modèle réel après re-sonde (matrice ARBITRAGE-05 §6).

---

## Session 06/09/2026 — Phase serveur : release v0.3.3, edge re-déployé, re-sonde VERTE

> Suite du journal. Tout exécuté sur les serveurs avec les helpers
> `SERVEUR/` (wonkom) et `MANIA.SN/.tmp-deploy/` (Mania) — jamais d'inline.

### Release v0.3.3 (wonkom, builds manuels)

- Commit `3c73888` publié : bundle→wonkom→`push origin main` (fast-forward).
- `release-v033.sh` (adapté de v0.3.2) : portes `cargo test --locked`
  workspace + `e2e_n0` 8/8 **exécutées** (jamais `cargo check` seul), builds
  linux-gnu + windows-gnu (mingw), macOS copiés de v0.3.2 (caveat runners),
  tag `v0.3.3` poussé, draft créé (`release_id=383526345`).
- **Leçon v0.3.2 récidivée 1** : la résolution du `release_id` par
  `GET /releases?per_page=5` ne voit pas le draft → reprise
  `rel-v033-resume.sh` (liste `per_page=100` + repli sur l'ID candidat vérifié
  par son tag — procédure éprouvée du 04/09).
- **Leçon v0.3.2 récidivée 2 (la vraie)** : `n0-release-assets.sh` calculait
  encore les checksums AVANT l'upload bundle+libs → 4 entrées au lieu de 8,
  installateurs KO — le commentaire « uploads AVANT checksums » existait mais
  le code n'avait jamais été réordonné. Réparé à la main
  (`rel-v033-fixchecks.sh`, 8 entrées) **puis corrigé pour de bon dans le
  dépôt** (réordonnancement §3/§4, commit de cette session).
- Release v0.3.3 complète : 9 assets, checksums 8 entrées, downloads publics
  200. Smoke Windows du binaire **publié** (sha256 vérifié, rapatrié via
  wonkom) : sonde locale verte (0 échec NER long message, zéro valeur réelle
  côté mock, restaurations exactes ; `[VILLE_SN]` = généralisation
  irréversible **par conception** — test `test_n0_policy_ville_sn_…`).

### Rebuild edge + re-déploiement demo-cloison (Mania)

- `rebuild-edge-v033.sh` : image `mania-cloison-edge` reconstruite pin
  v0.3.3 (checksums vérifiés), backup `backup-20260906-092209`, rollback
  documenté.
- `deploy-demo-edge-v033.sh` : `compose up -d` → **seul l'edge recréé**
  (agent/webui intacts). 0 échec NER au boot.

### 🔵 Cause racine des `UpstreamTimeout` (nouveau, pas dans le 05-06)

- Symptôme : requêtes réelles (prompt SOUL 6737 tokens) → `UpstreamTimeout`
  `error sending request` par paires à 5 s (connect_timeout), requêtes
  triviales OK. Réseau/DNS/IPv6/firewall/TLS **tous exonérés** (A/B edges
  jetables v0.3.2 ET v0.3.3 → 200 en <1 s ; curl dual-réseau OK ; tcpdump +
  conntrack OK).
- **Cause** : `mem_limit: 256m` + `cpus: 0.5` de l'edge dans le compose v4.1.
  En v0.3.2 le NER mourait instantanément sur les gros prompts (bug 512) donc
  ne coûtait rien ; en v0.3.3 le **fenêtrage travaille vraiment** (26 fenêtres
  × ONNX) → 240/256 MiB au repos, `memory.peak` = limite exacte, CPU saturé à
  50 % → le connect amont explose les 5 s. Preuve : `docker stats`
  94 % + `memory.peak` == limite.
- **Correctif** : tenant `mem_limit 1g` + `cpus 2.0` (backup compose horodaté)
  **+ gabarit vivant `nouveau-tenant.sh` patché pareil** (backup horodaté) —
  à reporter au dépôt ManIA.

### Clé composite : transitoire de rotation (WebUI)

- La WebUI synchronise le composite dans le `.env` home hermes ET le
  `CLOISON_TOKEN` du `.env` tenant — une re-saisie a fait osciller le jeton
  (A→B→A→B) pendant la session, d'où des 401 intermittents non liés à
  l'edge. Stabilisé sur le composite original après re-saisie pilote ;
  vérification par empreintes uniquement (jamais de clé affichée).

### Re-sonde réelle VERTE (DeepSeek direct)

- `hermes -z` roundtrip PII synthétique : réponse complète, **restaurations
  exactes** (Aminata Diop, email, téléphone ; ville = `[VILLE_SN]` par
  conception), **zéro valeur réelle dans les logs edge** (Aminata/Diop/email/
  tél/Ziguinchor = 0), **0 échec NER** sur le vrai prompt 6737 tokens.
- Le modèle (deepseek-v4-flash servi en « deepseek-chat ») a **préservé les
  sentinelles** (restauration exacte) — à verser à la matrice
  ARBITRAGE-05 §6 (sentinelles par défaut tiennent sur DeepSeek direct).
- 🔴 `response_format` 400 toujours présent (3× pendant la sonde, Hermes
  retente et aboutit) — décision : strip côté edge (retry sans
  `response_format` sur ce 400 précis) ou config Hermes — **à trancher**.
