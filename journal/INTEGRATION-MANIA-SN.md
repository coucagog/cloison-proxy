# CLOISON × MANIA.SN — INTEGRATION : rapport de propositions

> Journal de développement — session fin août 2026. Statut : **propositions
> uniquement, rien n'a été modifié en production** (à la date de rédaction).
> Décision pilote actée en cours de session : **mania-pii est abandonné au
> profit de CLOISON**.
> Références : `Doc_REF/CLOISON-NOTE-TECHNIQUE.md`, `cloison/journal/STACK-*.md`,
> `MANIA.SN/STACK*.md`, `MANIA.SN/ARCHIVES/STACK-4-chantier-pii.md`,
> `MANIA.SN/pages2/cloison-topologie_PII.html`, gabarit
> `MANIA.SN/ManIA/services/gabarit/nouveau-tenant.sh`.

---

## 0. Cartographie officielle des serveurs (actée par le pilote)

| Projet | Adresse | Hôte | Notes |
|---|---|---|---|
| **MANIA.SN** | **`51.38.179.242`** | `vps-6dcf6a6b` (Debian 13) | prod Mania : Traefik, tenants Hermes, gabarit `/opt/hermes/gabarit` |
| **wonkom / CLOISON** | **`144.217.81.251`** | VPS OVH (user `debian`) | edge `api.wonkom.ai` · `journal.wonkom.ai` · `docs.wonkom.ai` · stack interne control/detect/postgres |

> Historique des bascules (pour lecture des vieux journaux) : Mania occupait
> autrefois `144.217.81.251` (`vps-fd0110d5`, OVH Montréal) puis a migré sur
> `51.38.179.242` ; wonkom occupait `51.38.179.242` puis a migré sur
> `144.217.81.251` (DEPLOY-1 CLOISON). **Aujourd'hui : Mania = 51.38.179.242,
> wonkom = 144.217.81.251.** Les deux stacks vivent donc sur deux hôtes
> distincts — à prendre en compte dans toute topologie d'intégration.
>
> NB : `MANIA.SN/PII_WONKOM.ai` et `MANIA.SN/cloison-wonkom.ai` ne sont PAS des
> journaux mais des **profils SSH Bitvise** (« Tunnelier 9.66 ») vers wonkom.ai
> (user `debian`) ; un profil SSH dédié « cloison » existe déjà pour y intervenir.

---

## 1. Constat : l'inventaire des flux LLM de MANIA.SN

**Il n'existe qu'un seul flux sortant vers un LLM en production : l'agent Hermes.**

| Composant | Appelle un LLM ? | Rôle |
|---|---|---|
| `nousresearch/hermes-agent` (1 conteneur par tenant) | **OUI — le seul** | agent → OpenRouter (puis DeepSeek V4 Flash, puis retour OpenRouter Management API) ; clé saisie par le client dans sa WebUI (§4quater : la plateforme ne détient jamais la clé client) |
| `mania-webui` (rebrand de `ghcr.io/nesquena/hermes-webui`, MIT) | Non | UI seule (port 8787), parle à l'agent sur `:8642` |
| App Next.js `ManIA` | Non | aucun SDK/fetch LLM ; `proxy.ts` = middleware d'auth Next.js, PAS un proxy réseau |
| `mania-transcription` (Whisper) / `mania-documents` (LibreOffice) | Non | STT / conversion de fichiers |
| `mania-provisiond` | Non | démon de provisioning (socket Unix) |

**Aujourd'hui, ces flux partent en clair.** Le chantier mania-pii (proxy
Python Presidio) avait été construit, déployé et prouvé (sonde verte §53,
egress §56), puis **reverté le 9 août** (commit `1a93d5c`) et **abandonné au
profit de CLOISON** (décision pilote). Les 13 `packs/*.conf` (dont `sante`,
`droit`, `finance`, `gouvernement`, `ong`) sont tous à `PII=0`. Le câblage
d'insertion, lui, **reste en place et est directement réutilisable** dans
`ManIA/services/gabarit/nouveau-tenant.sh`.

## 2. Le principe d'insertion (déjà validé par l'incident §55)

CLOISON s'intercale **entre l'agent Hermes et le fournisseur**, jamais devant
la WebUI. Le levier n'est pas un patch Hermes : c'est le **profil de
fournisseur natif** de `config.yaml`, que `nouveau-tenant.sh` sait déjà écrire
(lignes 525-603) :

```yaml
providers:
  mania-pii:               # → devient cloison
    base_url: http://<proxy>:<port>/v1
    key_env: OPENROUTER_API_KEY
    api_mode: chat_completions
model:
  provider: custom:mania-pii   # → custom:cloison
```

Pièges **documentés et déjà contournés** dans le script :
- Hermes #25107 : `model.base_url` est effacé par le sélecteur de modèle →
  le routage doit venir du profil, jamais d'une URL nue ;
- `hermes config set` journalise la valeur écrite (incident 55) → filtre de
  caviardage sur la forme `<slug>.<hex>` ;
- réseau fermé **avant** les opérations de config (sinon `hermes` reste pendu).

Les **quatre gestes indissociables** restent la doctrine : **réseau
`internal:true` + proxy raccordé au réseau tenant (`docker network connect`)
+ profil provider `custom:` + section `# Pseudonymisation` du SOUL**.

## 3. Pourquoi remplacer mania-pii par CLOISON

| Capacité | mania-pii (abandonné) | CLOISON |
|---|---|---|
| Marqueurs | `[NOM_1]` + `str.replace` naïf (un marqueur halluciné par le modèle serait restauré quand même) | sentinelles `⟦TYPE·corps·mac⟧` + **registre d'émission par requête + MAC** : on ne restaure que ce qui a été émis (anti-hallucination, invariant I3) |
| Tool-calls | **Non traités** — or les skills `remplir-gabarit`/`convertir-document` font passer les données par les arguments d'outils | `tool_calls[].function.arguments` masqués/restaurés aller-retour (prouvé STACK-3, stream compris) |
| Streaming | Refusé (contrainte imposée à Hermes) | SSE buffer-and-scan, sentinelle coupée entre chunks jamais fuitée |
| Détection | Presidio FR + spaCy + regex — noms ouest-africains mal couverts (« reconnaisseurs sénégalais vides », dette consignée) | regex + **gazetteers `nom_sn`/`ville_sn` + CNI Luhn + afroxlmr** : verdict GO mesuré — PERSON **0.937** vs 0.518 (baseline Presidio forte), LOC 0.835, macro 0.954, spécificité 77 % |
| Alias | Non | Expansion intra-session R1–R7 (« Mamadou » → « Momo » masqué aussi, jamais les pronoms) |
| Quasi-identifiants | Heuristique clinique locale | Jauge opt-in (« patient de 42 ans opéré le 3 mars à Ziguinchor » → signal de densité, jamais de prétention de résolution) |
| Preuve de conformité | Aucune | Reçus signés Ed25519 (compteurs, zéro texte) + rapports k-anonymes + journal de transparence — matière pour la CDP (loi 2008-12 / CEDEAO) |
| Empreinte serveur | FastAPI + spaCy (Python) | Distroless Rust, read-only, `cap_drop: ALL`, quelques Mo de RAM |

## 4. Les propositions (phases, du moins risqué au plus fort)

### Phase 0 — Audit d'abord (recommandée, zéro risque de casse)
Déployer **un** edge CLOISON en **mode audit** (`CLOISON_AUDIT_MODE=1`) et
pointer dessus 1-2 tenants ouverts (PII=0), sans fermer leur réseau. Le proxy
observe et compte : « X noms, Y CNI, Z mentions de santé sont partis vers
OpenRouter ce mois-ci ». Résultat : la **mesure de l'exposition actuelle par
verticale** (rapport k-anonyme signé présentable à la CDP) + validation du
routage Hermes→CLOISON **sans rien masquer**. C'est l'« Étape 1 · Audit » du
doc `pages2/cloison-topologie_PII.html`.

### Phase 1 — Substitution du proxy dans le gabarit (remplacement de mania-pii)
Dans `nouveau-tenant.sh` : `PROXY_PII="cloison-edge"`, port interne libre
(8787 en interne conteneur — pas de collision : le 8787 « WebUI » est un autre
conteneur). Trois options d'auth par tenant, à arbitrer :

1. **(Recommandé) Un edge par tenant.** `cloison-edge` ajouté au compose du
   tenant (image `ghcr.io/coucagog/cloison-proxy:edge`), réseau `internal`,
   `CLOISON_EXPECTED_ACCESS_TOKEN` = jeton dérivé du slug avec le même HMAC
   `SECRET_SHARED` que `SHARED_SERVICES_TOKEN` (patron existant,
   `nouveau-tenant.sh:190-196`). Isolation maximale, aucun header à injecter,
   révocable par tenant.
2. **Edge partagé + plan de contrôle.** Un seul edge, `cloison-control` en
   interne (8788), un tenant `mn_` par slug provisionné via
   `cloison-cli`/`onboard_client.sh`, header `X-Cloison-Tenant` injecté par
   Traefik (middleware `headers` par routeur `<slug>`). Ops plus léger, à
   valider empiriquement (même esprit que la sonde `OPENAI_BASE_URL`).
3. **Edge hébergé `api.wonkom.ai`** (N3) : zéro conteneur à gérer, mais c'est
   le niveau où l'éditeur lit le clair — à réserver aux tenants non sensibles,
   jamais comme argument.

Dans tous les cas, **la clé saisie par le client dans la WebUI devient la clé
composite** `mn_<jeton>.<clé_du_client>` — le pattern « deux champs » de
CLOISON. Effet de bord positif : le secret **sort de l'URL** (fin de la dette
« token dans l'URL / access-logs Traefik », pt 12/26) puisqu'il vit dans
`Authorization`.

**Ajustement indispensable** : la section SOUL `# Pseudonymisation`
(`nouveau-tenant.sh:324-360`) décrit les marqueurs `[NOM_1]`. À réécrire pour
les sentinelles `⟦…⟧` (mêmes règles : recopier à l'identique, ne jamais en
inventer, règle de silence). Le contrôle d'activation change aussi : vérifier
les **compteurs du edge** CLOISON au lieu du `grep PROBE` mania-pii, en gardant
la vérification egress (`curl https://1.1.1.1/` → exit 7).

### Phase 2 — Activation par verticale (opt-in, jamais global)
Passer `PII=1` d'abord sur `sante`, `droit`, `finance`, `gouvernement`,
`ong` — règle des journaux : *métiers à secret professionnel =
pseudonymisation obligatoire*. `--pii` reste le levier manuel pour élever un
cas hors liste (dossier RH dans « Services »). Chaque activation vérifiée avec
le triptyque existant (egress fermé + proxy 200 + profil sélectionné) **plus**
un `hermes -z "Bonjour"` avec contrôle des compteurs du edge.

Politique CLOISON à poser pour ces packs (`json_policy` par locataire) :
- **santé** : suppression pure CNI/carte bancaire (comme mania-pii),
  généralisation date→`YYYY-MM`, ville→`[VILLE_SN]`, **jauge quasi-id
  activée** (`CLOISON_QUASI_ID_GAUGE=1`) ;
- **`/v1/embeddings` : bloqué** — la mémoire vectorielle d'Hermes serait un
  second canal d'egress ; N0 le bloque par défaut, l'edge doit le bloquer par
  politique ;
- **Streaming** : ré-autorisable (Hermes l'avait désactivé pour mania-pii) ;
  CLOISON le gère.

### Phase 3 — Rappel maximal côté serveur (option)
Si la latence le permet, brancher le sidecar `cloison-detect` (afroxlmr +
GLiNER, image complète, `CLOISON_DETECT_URL`) pour le rappel PERSON/LOC des
paliers serveur (2-6 s/doc sur CPU, à mesurer — le CPU multi-tenant n'a jamais
été mesuré, dette consignée). Sinon, le **NER léger embarqué** de N0
(distilbert ONNX int8, ~11 ms/doc, PERSON +0.62) suffit déjà pour les noms
hors gazetteer.

### Phase 4 — Preuve et conformité (le différenciateur produit)
Audit opt-in sur les tenants PII (reçus signés persistés
`CLOISON_AUDIT_LEDGER_FILE`, ingest vers `cloison-control` → ledger) :
rapports k-anonymes **par tenant** pour la déclaration/autorisation CDP
(régime deux étages : la santé exige l'autorisation préalable — non engagée à
ce jour, dette consignée). C'est l'argument « preuve sans avoir vu » du doc de
topologie.

### Phase 5 — L'offre edge pour les métiers à secret professionnel (N0/N1)
La promesse absolue ne tient pas sur un VPS partagé : en hébergé, l'opérateur
*pourrait* lire (niveau N3, à dire honnêtement). Pour les clients qui
l'exigent : **stack locale N1** (CLOISON edge + Hermes agent sur la machine du
client) ou **daemon N0** (`install-n0.ps1`/`.sh`, ≤ 10 min) devant un agent
tournant chez eux — installation N0 prouvée sur Windows ; le module
`@cloison/core` (WASM) ouvre la voie à une app mobile MANIA pseudonymisant en
local.

### Bonus — le second flux LLM de l'écosystème
Le pipeline **Open Design** (daemon headless → agent `deepseek-harness` →
DeepSeek, génération du site docs.wonkom.ai) est un autre egress LLM : le
daemon N0 y a déjà été prouvé de bout en bout (`journal/E2E-OPEN-DESIGN.md`)
— même recette, même deux champs.

### Niveaux de cloisonnement de chaque option (à ne JAMAIS confondre)

| Option d'intégration | Où tourne le moteur | Niveau CLOISON | Qui peut lire le clair ? |
|---|---|---|---|
| Edge hébergé sur le VPS Mania (phases 0-4) | VPS Mania `51.38.179.242` | **N3 hébergé** | l'opérateur (Mania) le pourrait ; le fournisseur LLM ne voit que des jetons |
| Edge partagé `api.wonkom.ai` | VPS wonkom `144.217.81.251` | **N3 hébergé** | idem, plus un opérateur tiers (wonkom) |
| Stack locale chez le client (agent + edge) | serveur du client | **N1 site** | seul le client, chez lui |
| Daemon N0 + agent local chez le client | poste du client | **N0 local** | personne (hors poste compromis) |
| App mobile `@cloison/core` (WASM, in-memory) | app du client | style N0 | personne |

> **Conséquence actée** : le câblage MANIA en hébergé = **N3**, dit et assumé.
> Les mitigations (réseau `internal`, mapping éphémère, zéro log PII, reçus
> signés) ne changent pas le niveau — elles réduisent les fuites accidentelles
> et prouvent le masquage. N1/N0 exigent un déploiement chez le client
> (phase 5), et restent la seule offre pour les métiers à secret professionnel
> (règle déjà consignée dans les journaux MANIA).

## 5. Risques et points à valider (honnêteté charte §11)

1. **UX clé composite** : le client colle `mn_….<sa clé>` au lieu de sa clé nue
   — à documenter dans le rapport d'activation de `nouveau-tenant.sh`.
2. **Routage multi-tenant par header** (option edge partagé) : non validé côté
   Hermes/Traefik — à prouver par une sonde avant généralisation.
3. **Périmètre honnête** : quasi-identifiants (signalés, pas résolus), PII
   hallucinée sans jeton (grounding), poste compromis. Ne jamais vendre
   « anonymisation » là où CLOISON fait de la **pseudonymisation réversible**
   (pour le LLM qui ne peut pas inverser, c'est anonyme — jurisprudence
   EDPS c. CRU, citée dans le doc de topologie).
4. **Niveau de confiance à afficher** : edge sur le VPS Mania = N3
   (l'opérateur pourrait lire) ; par-tenant + coffre non persisté + zéro log
   PII réduit la surface mais ne change pas le niveau. N1/N0 seuls portent la
   promesse absolue.
5. **Gabarit vivant ≠ dépôt** (dette consignée) : synchroniser
   `/opt/hermes/gabarit` avec `ManIA/services/gabarit` **avant** tout câblage
   CLOISON, sinon on propage un script fantôme.
6. **Hôtes distincts** : Mania (51.38.179.242) et wonkom (144.217.81.251) sont
   deux machines — toute option « edge partagé » doit trancher l'hôte
   d'hébergement du edge (recommandé : côté Mania, dans le réseau docker des
   tenants, pour le verrou egress).

## 6. 🔴 Alertes sécurité (indépendantes de CLOISON, à traiter en priorité)

- `MANIA.SN/SERVEUR/SERVEUR_mania.sn.txt` contient des **secrets en clair**
  (token GitHub, clé AWS/Backblaze, clé de chiffrement, passphrase SSH) →
  considérer comme compromis, **révoquer/regénérer** et sortir du dépôt.
- `MANIA.SN/MANIA.SN_RIDWAN_HERMES_CONFIG` contient une **clé OpenRouter en
  clair** → même traitement.

## 7. Fichiers clés

| Fichier | Rôle |
|---|---|
| `MANIA.SN/ManIA/services/gabarit/nouveau-tenant.sh` | provisioning v3 : packs, 4 gestes PII, profil `custom:mania-pii` (lignes 525-603), vérifications |
| `MANIA.SN/ManIA/services/gabarit/packs/*.conf` | `PII=0|1` par verticale (13 packs, tous à 0) |
| `MANIA.SN/ManIA/services/gabarit/SOUL.gabarit.md` | SOUL des agents (+ section `# Pseudonymisation` posée par le script) |
| `MANIA.SN/PII/` | sources mania-pii (abandonné) : `main.py`, `pii_engine.py`, `presidio_adapter.py` |
| `MANIA.SN/pages2/cloison-topologie_PII.html` | doc de topologie CLOISON (niveaux N0-N3, clé composite, roadmap Audit d'abord) |
| `cloison/crates/cloison-proxy/` | la passerelle OpenAI-compatible (AGPL-3.0) |
| `cloison/deploy/docker-compose.dev.yml` + `Dockerfile.proxy` | déploiement edge Docker (distroless, read-only) |
| `cloison/docs/N0.md` + `deploy/install-n0.sh|ps1` | daemon local N0, installation ≤ 10 min |
| `cloison/deploy/onboard_client.sh` | onboarding locataire N3 (jeton `mn_` + clé composite) |

## 8. Prochaines étapes (en attente d'arbitrage pilote)

> **MAJ 02-03/09/2026** : la **sonde E2E est VERTE** — tenant Hermes jetable
> (`sonde-cloison`) → edge CLOISON (binaire release, conteneur) → mock LLM :
> le mock n'a reçu **que des sentinelles** ⟦…⟧ (aucune PII), l'agent a reçu
> les valeurs restaurées, egress verrouillé (curl 7), 401 fail-closed, SSE
> prouvé. Dépouillement complet, état initial restauré et vérifié. Détail,
> runbook et 9 découvertes produits à escalader :
> `journal/E2E-MANIA-TENANT.md`. Manuel v2 (volets, clique-pour-copier) et
> scripts `deploy/configure-n0.*` livrés dans le dépôt.

1. Arbitrer les **découvertes produits** (image GHCR non publique, crash
   N0+audit, mock_mode, clamp k, bundle NER VPS) — cf. journal E2E.
2. **Phase 1** : patch du gabarit (`nouveau-tenant.sh` → `custom:cloison` +
   clé composite + section SOUL ⟦…⟧), dans le dépôt d'abord, jamais direct
   sur `/opt/hermes/gabarit`.
3. Sonde avec **clé réelle** (OpenRouter/GLM) : même runbook, partie amont
   réelle.
4. Déploiement du manuel sur `docs.wonkom.ai` (acte séparé).

---
*Fin du rapport. Aucune modification de production n'a été faite à ce stade.*
