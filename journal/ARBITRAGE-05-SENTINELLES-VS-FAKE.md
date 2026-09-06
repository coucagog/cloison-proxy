# CLOISON — ARBITRAGE-05 : sentinelles ⟦…⟧ vs faux réaliste (par tenant/verticale)

> Document d'aide à la décision **pilote** — les faits sont mesurés. La
> **politique générale a été actée par MLS le 05/09/2026** (voir §6) ; l'opt-in
> « faux réaliste » reste décidé **par tenant**, sur mesures.
> Rédigé le 04/09/2026, après la publication de la release **v0.3.2** qui rend
> l'option `CLOISON_REALISTIC_FAKE` effective (gabarit v4.1 : pass-through par
> tenant, défaut `0` = sentinelles). Décision consignée le 05/09/2026 (§6).
> Références : `journal/E2E-MANIA-TENANT.md` (sondes 02-03/09 et 04/09),
> `journal/INTEGRATION-MANIA-SN.md` §4, `crates/cloison-core/src/fake.rs`,
> `docs/CONFIG.md`, charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` §16.

## 1. Objet

Les tenants MANIA `PII=1` (sante, droit, finance, gouvernement, ong) ont
désormais un edge CLOISON par tenant. Deux stratégies de restitution sont
disponibles ; elles s'excluent par session/tenant, et la doctrine NEXT-SESSION
est : **ne jamais mélanger sans vérifier la restauration complète**.

## 2. Faits mesurés (pas d'opinion)

| | **Sentinelles ⟦…⟧** (`CLOISON_REALISTIC_FAKE=0`, défaut) | **Faux réaliste** (`=1`) |
|---|---|---|
| Principe | jeton opaque ⟦type·corps·mac⟧ ; restauration **exacte** à partir du registre d'émission | substitution **déterministe** (par session, via le corps du jeton) ; **irréversible** |
| Preuves | sonde E2E 02-03/09 (sentinelles seules chez le mock, restauration client identique) ; smoke Windows exit 0 | sonde 04/09 sur le VPS Mania : le « fournisseur » a reçu `Maimouna Yacine, +221 77 256 51 21, [VILLE_SN]`, zéro PII, zéro sentinelle |
| Types couverts | tous (masquage complet) | PERSON, Gazetteer(nom_sn), PhoneSn, Email — **repli sentinelle** pour les autres types ; ville toujours généralisée `[VILLE_SN]` |
| Restauration | **exacte** (valeurs réelles côté client) | **faux** côté client (assumé, par design) |
| Modèles qui dépouillent `⟦…⟧` (deepseek-v4-flash constaté) | 🔴 dégradation : corps bruts côté client (pas de fuite — HMAC irréversibles sans la clé du coffre) | ✅ **indifférent** : il n'y a plus de sentinelle à dépouiller |
| Flux documentaires / tool-calls (`remplir-gabarit`, `convertir-document`, arguments d'outils) | ✅ valeurs **réelles** dans le document produit | 🔴 **faux dans le document produit** — un devis, un contrat, une fiche patient porteraient un nom/téléphone inventé |
| Traçabilité/audit | reçus signés, compteurs (les deux modes partagent le même ledger) | idem ; en plus, le faux est réversible **uniquement** par rejeu du coffre (jamais en prod) |

## 3. Critères de choix (à appliquer par tenant)

1. **Le flux dominant.** Conversationnel (chat, vocaux, synthèses) → les deux
   conviennent. Documentaire (gabarits, conversions, envois) → **sentinelles
   obligatoires** : un faux réaliste entrerait dans un document officiel sans
   que rien ne le signale.
2. **Le modèle servi.** S'il dépouille `⟦…⟧` (mesuré sur deepseek-v4-flash) et
   que le flux est conversationnel → faux réaliste. Sinon → sentinelles.
3. **La verticale.** Santé/droit : la restitution exacte d'un nom ou d'une date
   dans un compte rendu peut être une exigence métier → tendance sentinelles.
   ONG/communautaire (correspondance de masse) : le faux peut suffire.
4. **L'irréversibilité.** Le faux réaliste est irréversible par design ; le
   client qui découvre un faux dans sa réponse doit le comprendre — donc
   **communiquer le mode choisi au client** (une ligne dans l'onboarding).

## 4. Recommandation (pour arbitrage MLS)

- **Défaut conservé : sentinelles** (`=0`) — c'est le seul mode qui préserve
  les flux documentaires et la restitution exacte.
- **Faux réaliste en opt-in**, tenant par tenant, pour les seuls cas où (a) le
  modèle servi dépouille les sentinelles ET (b) le flux est conversationnel.
- Pose : `CLOISON_REALISTIC_FAKE=1` dans le `.env` du tenant (0600), `docker
  compose up -d` de son edge (recréation — les tenants existants PII=0 ne sont
  pas concernés), puis **sonde de restauration complète** avant de déclarer OK
  (critère NEXT-SESSION item 2).
- **Jamais** : activer le faux sur un tenant qui produit des documents via les
  skills mania, sans avoir vérifié que ces flux ne passent pas par l'edge avec
  des arguments d'outils porteurs de PII.

## 5. Ouvert (à trancher un jour, hors urgence)

- Faux réaliste **périmétré par flux** (conversation seulement, jamais les
  tool-calls) : variante produit possible (flag `CLOISON_REALISTIC_FAKE_TOOLS=0`
  à spécifier) — piste, non implémentée.
- Détection automatique du dépouillement de sentinelles (heuristique sur le
  flux de réponse) pour suggérer la bascule — piste, non implémentée.

---

## 6. ✅ DÉCISION MLS (05/09/2026) — politique par verticale

> Validée par le pilote le 05/09/2026. Ce qui est figé ici est la **politique
> générale** ; l'opt-in reste décidé **par tenant**, sur les 4 conditions
> mesurées ci-dessous.

- **Défaut partout : sentinelles** (`CLOISON_REALISTIC_FAKE=0`). **Aucun
  changement en production** : le défaut est déjà `0` dans le gabarit v4.1
  (vérifié : `nouveau-tenant.sh` L331, repo `ManIA` commit `6d4d96c`), et
  aucun tenant en service n'est `PII=1` (`ridwan`/`skd`/`agnes` provisionnés
  avant l'activation du 03/09). Section SOUL « # Pseudonymisation » posée au
  provisioning (L394-436) : l'agent traite `⟦…⟧` comme la valeur, n'en parle
  jamais.

- **Matrice par verticale (packs `PII=1`)** :

  | Verticale | Flux dominant | Politique |
  |---|---|---|
  | `sante` | documentaire (comptes rendus, fiches) | **Sentinelles** |
  | `droit` | documentaire (contrats, actes) | **Sentinelles** |
  | `finance` | documentaire (devis, factures) | **Sentinelles** |
  | `gouvernement` | documentaire (actes administratifs) | **Sentinelles** |
  | `ong` | mixte (correspondance de masse) | **Sentinelles par défaut** ; faux = opt-in par tenant |

- **Opt-in « faux réaliste »** — uniquement si les 4 conditions sont
  **mesurées** sur le tenant concerné : (a) le modèle servi dépouille `⟦…⟧` ;
  (b) flux conversationnel dominant ; (c) aucun flux documentaire via les
  skills mania ; (d) client informé de l'irréversibilité (ligne d'onboarding
  AVANT la mise en service).

- **Procédure d'application** (le jour où un tenant le justifie) : décision
  MLS → sauvegarde `.env.bak-<stamp>` → pose `CLOISON_REALISTIC_FAKE=1` dans
  `/opt/hermes/<slug>/.env` (0600, **fichier script** scp'é, jamais inline) →
  `docker compose up -d` (l'edge seul est recréé) → **sonde de restauration
  complète** (kit `SERVEUR/` : `mock-llm.py` + `sonde-phase*.sh` /
  `probe-fake-v032.sh` adaptés à l'edge du tenant ; preuves par appels réels :
  zéro PII/sentinelle chez le fournisseur, faux déterministes côté client,
  tous les types couverts y compris les replis sentinelles) → journaliser →
  reverser au dépôt si le gabarit bouge.

- **Nuance retenue** : le faux ne couvre que PERSON / Gazetteer(nom_sn) /
  PhoneSn / Email ; les autres types restent en sentinelles (ville →
  `[VILLE_SN]`). Sur un modèle dépouilleur, le faux ne supprime donc pas
  **toute** dégradation — la condition (a) se mesure sur le tenant, pas en
  laboratoire.

- **Dettes de cohérence consignées** (dépôt ↔ gabarit, cosmétiques, à corriger
  au prochain commit du gabarit) : `services/gabarit/README.md` dit encore
  « v3 » et l'en-tête du compose généré « v2 » alors que le script est v4.1.

- **Fait mesuré à verser (06/09, re-sonde réelle `demo-cloison`)** : sur
  DeepSeek **direct** (`deepseek-chat` servi par l'edge v0.3.3.1), le modèle
  **préserve les sentinelles** — restauration exacte prouvée (`Aminata Diop`,
  email, téléphone restitués ; ville `[VILLE_SN]` par conception). Le
  dépouillement mesuré les 02-03/09 concernait `deepseek-v4-flash` servi via
  OpenRouter. → Le défaut « sentinelles » **tient** sur cette configuration ;
  l'opt-in faux réaliste ne se justifie que si un dépouillement est MESURÉ
  sur le tenant concerné (condition (a) de la politique).
