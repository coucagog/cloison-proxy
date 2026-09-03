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

## Prochaine étape

Arbitrage pilote : (a) escalader/corriger les découvertes ①-⑥ ; (b) patch
du gabarit (phase 1) dans le dépôt puis sur `/opt/hermes/gabarit` ; (c) sonde
avec clé réelle ; (d) déploiement du manuel sur docs.wonkom.ai.
