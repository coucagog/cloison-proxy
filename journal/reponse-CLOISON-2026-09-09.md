# Rapport entrant + recommandations de câblage — CLOISON

> Fichier d'origine : `cloison/journal/rapport-CLOISON-2026-09-09.md` (retour
> client, copié intégralement ci-dessous). Assorti de **recommandations** à
> suivre par cet agent (l'équipe d'évaluation) pour atteindre son objectif :
> câbler ses agents de codage locaux sur CLOISON N0, sans PII en clair chez
> le fournisseur et avec des réponses exploitables.

---

# Rapport technique — CLOISON (proxy de pseudonymisation PII)

**À l'attention des équipes CLOISON**
**Date :** 9 septembre 2026
**Objet :** Retour de validation PoC N0 v0.3.x, questions précises et points d'attention avant intégration dans notre flux d'agents de codage locaux.

---

## 1. Contexte & cas d'usage

Nous évaluons CLOISON pour protéger les données transitant vers les **agents de codage locaux** utilisés sur nos postes de travail (distribution Linux « Omarchy », Hyprland). L'objectif : qu'**aucune donnée personnelle identifiable ne quitte nos machines en clair** vers les fournisseurs de modèles, tout en gardant des réponses exploitables (restauration à la réponse).

- Cible principale envisagée : **Claude Code** (protocole Anthropic), sur des sessions longues d'agents (nombreux allers-retours, appels d'outils, contextes volumineux).
- Cible alternative : agents OpenAI-natifs (Codex, opencode).
- Cadre juridique de référence : loi sénégalaise n° 2008-12, Acte additionnel CEDEAO A/SA.1/01/10, Convention de Malabo 2014, régime CDP ; RGPD en complément pour nos marchés exposés.

## 2. Ce que nous avons validé (PoC local, aucune donnée sortie de la machine)

Test effectué en **N0** : binaire `v0.3.x` (Linux x86_64), NER ONNX int8 + onnxruntime embarqués, checksums SHA-256 vérifiés, daemon sur `127.0.0.1:8787`, fournisseur **simulé** en local.

| # | Constat | Détail |
|---|---|---|
| V1 | Pseudonymisation à l'aller | « Aminata Diop / user@example.com / +221 77 123 45 67 » → le fournisseur n'a reçu **que des jetons** ⟦…⟧ typés (gazetteer noms, email, téléphone) |
| V2 | Restauration au retour | Réponse restituée avec les vraies valeurs ; aucune sentinelle résiduelle |
| V3 | Coréférence | Même valeur → même jeton au sein d'un échange |
| V4 | Appels d'outils | `tool_calls` (arguments) **et** messages `tool` (résultats) pseudonymisés ; validité JSON préservée |
| V5 | Streaming | Support présent dans le code (`src/stream.rs`) — **non testé empiriquement** de notre côté (voir Q4) |
| V6 | Audit | `CLOISON_AUDIT_MODE=true` → `GET /v1/audit/report` disponible (rapport signé, compteurs par type, k=5) — sémantique à clarifier (voir Q10–Q12) |
| V7 | Auth | Clé composite `Bearer mn_<jeton>.<clé_amont>` (comparaison à temps constant), clé amont relayée telle quelle au fournisseur |

## 3. Questions précises

### A. Protocole & intégration des agents

**Q1 — Endpoint Anthropic.** Exposez-vous (ou prévoyez-vous) un endpoint `/v1/messages` compatible Anthropic ? Notre cible **Claude Code** se branche via `ANTHROPIC_BASE_URL` et parle exclusivement le protocole Anthropic ; votre routeur ne comporte aujourd'hui que des routes OpenAI (`src/routes.rs`). Si non prévu à court terme : quelle est votre position sur les passerelles de traduction tierces (LiteLLM, claude-code-router, claude-shadow) placées **devant** votre proxy ?

**Q2 — API Responses (OpenAI).** Un support de `/v1/responses` (utilisé par les agents récents, ex. Codex en mode natif) est-il au programme ?

**Q3 — Gestion de la clé amont.** La clé composite transporte la clé amont **en clair dans chaque requête**. Existe-t-il un mécanisme de référencement / coffre (au lieu de la clé elle-même), une rotation sans redémarrage, ou un multi-fournisseurs par politique ?

**Q4 — Restauration en streaming.** Pouvez-vous confirmer le comportement exact de la restauration sur flux SSE : découpage d'un jeton ⟦…⟧ entre deux chunks, rôle de `CLOISON_STREAM_NEUTRAL_MARKER` / `CLOISON_STREAM_MAX_TOKEN_LEN` / keep-alive, latence ajoutée ? Acceptez-vous un test conjoint d'un scénario agent (streaming + tools) ?

**Q5 — Contenu non textuel.** Seules les parts `text` sont tokenisées (constat dans `src/openai.rs`) : confirmez-vous que les images/audio et champs inconnus traversent **intacts** ? Qu'en est-il des champs émergents type `reasoning_content` (raisonnement étendu) ?

**Q6 — Sessions longues.** Pour un agent qui enchaîne des centaines d'appels avec des contextes de 100–200k tokens : la coréférence tient-elle sur toute la durée d'une session ? Quel est le TTL par défaut du vault (`CLOISON_VAULT_TTL_SECS`) et que se passe-t-il en cas de **redémarrage / rotation du vault en cours de session** (jetons antérieurs définitivement non restaurables ?) ?

**Q7 — Détecteurs et politique locale.** Quelle est la **liste exhaustive** des entités détectées en N0 et comment configurer une politique locale (activer/désactiver un détecteur, seuil NER `CLOISON_NER_THRESHOLD`, valeur de `k`, ajout de gazetteers métier) ? La doc produit présente un `config` avec `[fields] pseudonymise = ["email","phone","name"]` : à quel niveau s'applique-t-elle (N0 local, N1, hébergé) ?

**Q8 — Faux positifs sur du code.** Nos données sont majoritairement du **code et du contenu technique** (identifiants de variables, noms de produits, adresses fictives de tests). Quels mécanismes existe-t-il pour limiter les faux positifs (whitelist, lexique par projet/tenant) et évaluer l'impact sur la qualité des réponses ?

**Q9 — Généralisation.** Listez les règles de généralisation (date→`YYYY-MM`, IP→`[IP]`, carte→`••••`, ville→`[VILLE_SN]`…) et leur niveau de configuration.

**Q10 — Observe-only.** La FAQ documente un mode « observe-only : rien n'est transformé, tout est journalisé ». **Comment l'activer en N0 v0.3** ? Nous n'avons identifié que `CLOISON_AUDIT_MODE` (booléen). Un mode non-transformant avec journalisation locale est-il disponible ?

**Q11 — Rapport d'audit.** Sur notre test, `/v1/audit/report` renvoyait `"publishable": false` et des compteurs `redacted` à 0 malgré des masquages constatés. Pouvez-vous préciser la sémantique (fenêtre d'ingestion, condition de publication, rôle de `CLOISON_AUDIT_K`, « masked_by_type » vs « redacted ») et **à partir de quand un rapport est publiable tel quel** auprès d'une autorité (CDP) ?

**Q12 — Ledger local.** `CLOISON_AUDIT_LEDGER_FILE` n'a produit **aucune ligne** lors de notre test : que journalise-t-il exactement (reçus signés ? événements ?) et comment le **rejouer avec `cloison-verify`** en local (le journal public `journal.wonkom.ai` concerne-t-il uniquement l'hébergé ?) ?

**Q13 — Sorties réseau par défaut.** En N0, qu'est-ce qui quitte réellement la machine par défaut (aucun reçu ? compteurs ? télémétrie de version ?) et comment le garantir opérationnellement (`CLOISON_CONTROL_URL` / `CLOISON_DETECT_URL` non posés) ?

**Q14 — Performance sur gros contextes.** Coût CPU/temps de la détection (NER int8) par requête et latence ajoutée selon la taille (10k / 100k tokens) ? Recommandations d'exécution (daemon dédié, threads) pour un service user sur poste Omarchy/Arch ?

**Q15 — Vault & secrets.** L'intégration keychain Linux (Secret Service) est-elle confirmée ? Procédure de sauvegarde/restauration du vault et du sel de session (perte ⇒ jetons irrécupérables ?), et de **rotation de la tenant key sans perte** de coréférence ?

**Q16 — Limite de corps.** Valeur par défaut et maximum conseillé de `CLOISON_MAX_BODY_BYTES` pour des requêtes agent multi-fichiers (payloads de plusieurs Mo) ?

**Q17 — Artefacts publics.** Le dépôt public (`cloison-proxy`) est en AGPL-3.0 ; le monorepo et le corpus (gazetteers, jeux d'évaluation) sont privés/propriétaires. Quels artefacts serviront un client **N0 autonome** (vérificateur WASM local, `cloison-core`, `cloison-verify`…) et quelles sont leurs licences respectives ? Quel est l'impact d'une intégration du proxy dans une stack interne propriétaire ?

**Q18 — Version & cycle.** Version testée : `v0.3.x`. Rythme de release, canal d'annonce, politique de compatibilité de configuration entre versions ?

**Q19 — Position contractuelle.** En vue d'une mise en production (données Sénégal/UEMOA, possiblement UE) : fournissez-vous un **contrat de traitement / DPA**, un registre, et une analyse du régime CDP applicable à notre cas (proxy local pseudonymisant des prompts d'agents) — déclaration ou autorisation préalable ?

**Q20 — Anonymisation irréversible.** La pseudonymisation est réversible par conception. Face à des exigences d'**anonymisation vraie** (irréversible), un mode sans restauration est-il prévu ou envisageable ?

## 4. Points d'attention (identifiés de notre côté)

1. **Incompatibilité de protocole Claude Code** : absence de `/v1/messages` ⇒ passage par un adaptateur tiers (surface de confiance et de maintenance supplémentaires) ou choix d'un agent OpenAI-natif. À trancher avec vos réponses (Q1).
2. **N0 ne protège pas contre un poste compromis** : le coffre chiffré et la clé vivent sur la machine (limite assumée et documentée — nous l'intégrons à notre analyse de risque).
3. **Champs non déclarés = non masqués** : nécessité d'un **audit initial** (idéalement observe-only, Q10) et de revues périodiques à mesure que de nouvelles entités apparaissent dans nos flux.
4. **Faux positifs sur contenu technique** : risque de dégradation de la qualité des réponses de l'agent (identifiants, noms de produits) — nous prévoyons un comparatif avant/après.
5. **Corpus non auditable** : NER et gazetteers propriétaires — acceptable pour un usage sensible uniquement si la politique et les limites sont documentées précisément (Q7, Q9).
6. **Sessions longues d'agents** : dépendance à la durée de vie des jetons/vault (Q6) — un agent coupé/redémarré en cours de session pourrait recevoir des jetons non restaurables.
7. **Secrets** : la clé composite contient la clé amont en clair ; en N0 sur loopback le risque est limité, mais toute montée en N1 exige TLS et un plan de gestion des secrets (Q3, Q15).
8. **Hébergé (N3) = clair visible par l'opérateur** (assumé par votre doc) : nous privilégions donc **N0/N1** pour nos données.
9. **Journal & rapports** : la sémantique des compteurs et la rejouabilité locale restent à clarifier avant de pouvoir produire des preuves publiable (Q11, Q12).

## 5. Nos attentes pour la suite

1. **Réponses à Q1–Q20** (ou au minimum Q1, Q4, Q6, Q7, Q10–Q12, Q19) pour valider l'adéquation produit à notre cas « agent de codage local ».
2. **Session technique conjointe** (30 min) ou **scénario test encadré** : un agent de codage (Claude Code via adaptateur, ou Codex en direct) traversant le proxy en N0 sur une session réelle, avec streaming et outils — nous fournissons l'environnement de test.
3. **Accès à la documentation N0 complète** (variables, modes, limites) si elle n'est pas publique.
4. **Éléments contractuels** (DPA, registre, analyse du régime CDP) en vue d'une éventuelle mise en production.
5. **Feuille de route** : `/v1/messages` (Q1), `/v1/responses` (Q2), mode observe-only local (Q10), anonymisation irréversible (Q20).

Nous restons à votre disposition pour tout complément d'information sur notre environnement de test (binaire v0.3.x, scripts de PoC, journalisation des échanges simulés).

**Références consultées :** [docs.wonkom.ai](https://docs.wonkom.ai) (accueil, produit, install N0, API, FAQ), dépôt public [coucagog/cloison-proxy](https://github.com/coucagog/cloison-proxy) (`src/routes.rs`, `src/openai.rs`, `src/auth.rs`, `src/stream.rs`).

---

# Recommandations de câblage (rédigées côté CLOISON — à suivre par l'agent évaluateur)

> Objectif : câbler un agent de codage local (Claude Code ou Codex/opencode)
> sur CLOISON **N0**, avec zéro PII en clair chez le fournisseur et des
> réponses restaurées. État des lieux fondé sur le code, les tests e2e et les
> mesures réelles du 06/09/2026 (edge v0.3.3.1 en prod sur le tenant
> `demo-cloison`, DeepSeek direct).

## R1. Choisir le chemin d'intégration par cible

1. **OpenAI-natif (Codex, opencode) — chemin direct, recommandé en premier** :
   `base_url = http://127.0.0.1:8787/v1`, clé = composite
   `mn_<jeton>.<clé_amont>`. Aucun adaptateur, surface minimale. Les routes
   `/v1/chat/completions`, `/v1/models` existent et sont prouvées (tests e2e
   12/12 + prod réelle).
2. **Claude Code (protocole Anthropic) — passer par un adaptateur local** :
   CLOISON n'expose pas `/v1/messages` aujourd'hui (Q1 → feuille de route).
   Position : un adaptateur local (LiteLLM ou claude-code-router) avec
   `ANTHROPIC_BASE_URL` pointé dessus, et l'adaptateur configuré en amont
   OpenAI vers `http://127.0.0.1:8787/v1` + clé composite. L'adaptateur
   tourne en local : aucune donnée claire ne le quitte. À défaut, commencer
   par l'agent OpenAI-natif et brancher Claude Code quand `/v1/messages`
   sera priorisé.
3. **`/v1/responses` (Codex natif) : non exposé** (Q2). Forcer le mode
   chat/completions de l'agent, ou passer par l'adaptateur.

## R2. Provisionner N0 avec la VERSION ÉPINGLÉE v0.3.3.1

- Toujours `install-n0.sh --version v0.3.3.1` (jamais `latest` : le bundle
  NER `latest` a été incohérent sur Linux — historique documenté). Checksums
  SHA-256 **vérifiés par l'installeur** : exiger qu'ils passent, sinon
  signaler (leçon v0.3.2 : 4 entrées au lieu de 8 = installeur KO).
- v0.3.3.1 porte les correctifs critiques pour un usage agent :
  **fenêtrage NER** (les prompts système volumineux ne tuent plus la
  détection — le bug `512 by N` des v0.3.2 et antérieures) et la
  **dégradation `response_format`** (un 400 DeepSeek-direct est réessayé une
  fois sans `response_format` — plus de triple latence).
- `CLOISON_AUDIT_MODE=0` = masquage actif (⚠️ `=1` est l'observe-only, pas
  l'inverse — piège documenté).

## R3. Commencer par une passe « observe-only » (données de code)

- Vos flux sont du code : faux positifs certains (noms de variables, produits,
  adresses de test). **Activer `CLOISON_AUDIT_MODE=1`** (observe-only :
  détecte et compte SANS transformer) sur des sessions représentatives,
  exploiter `/v1/audit/report`, et constituer la liste des faux positifs
  récurrents (lexique projet/tenant — Q8 à préciser côté produit).
- Ensuite seulement, repasser en masquage actif et faire le **comparatif
  avant/après** de qualité des réponses que vous prévoyez (point 4 du
  rapport) : c'est la bonne méthode.

## R4. Sessions longues : vault persistant + sauvegardes + pas de rotation

- Le vault N0 est **persistant** (`ttl_s=604800` = 7 j par défaut) : un
  redémarrage du daemon NE perd PAS les jetons tant que vault + sel +
  passphrase sont intacts. Ce qui casse la restauration : suppression du
  vault, perte de la passphrase, ou **rotation de la tenant key en cours de
  session** (nouveau sel ⇒ jetons antérieurs irrécupérables).
- Pour un agent multi-jours : sauvegarder `vault.redb` + `.salt` ensemble,
  garder la passphrase hors de l'agent, et **ne jamais** régénérer la clé
  composite en cours de session. Coréférence intra-session : prouvée
  (V3 + tests), elle suit la durée de vie du vault (Q6).

## R5. Performance 100–200k tokens : MESURER avant d'engager

- Chiffres réels du 06/09 : ~26 fenêtres d'inférence ONNX pour un prompt de
  6 700 tokens (2 cœurs de VPS). À 100k tokens ⇒ ~400 fenêtres par requête :
  attendre des **dizaines de secondes d'aller** sur un poste de dev. C'est le
  facteur limitant n°1 de votre cas d'usage — à valider sur Omarchy avant
  d'engager (Q14). Pistes : daemon dédié sans limite CPU (la leçon du tenant
  Mania : 0,5 CPU/256 Mo sature — prévoir de la marge), et priorisation
  produit d'un mode « fenêtres parallèles » si la mesure est insuffisante.

## R6. Streaming + outils : déjà prouvé — réutiliser nos scénarios

- Réponse directe à Q4 : la restauration SSE est **testée** (découpage d'une
  sentinelle entre deux chunks → réassemblage ; sentinelle tronquée à la
  clôture → marqueur neutre `CLOISON_STREAM_NEUTRAL_MARKER`, fail-loud ;
  keep-alive configurable). Les tool_calls en streaming (id/name/arguments
  fragmentés) sont restaurés (V4 + tests). Scénario conjoint agent
  (streaming + tools) : accepté — votre offre d'environnement de test est la
  bonne porte d'entrée.

## R7. Corrections à intégrer dans votre analyse (le rapport sous-estime 3 points)

1. **Q20 — l'irréversibilité EXISTE** : `CLOISON_REALISTIC_FAKE=1` (v0.3.2+)
   = substitution réaliste **irréversible par session** (jamais la valeur
   réelle). Conditions d'usage mesurées : voir la matrice
   ARBITRAGE-05 §6. C'est votre levier si l'anonymisation vraie est exigée.
2. **Q10 — l'observe-only existe** : c'est `CLOISON_AUDIT_MODE=1`. Le défaut
   est documentaire (FAQ ambiguë), pas produit.
3. **Point 5 — le modèle NER est PUBLIC** : checkpoint Davlan/distilbert
   (licence AFL-3.0, NOTICE dans le bundle). Les gazetteers (listes de
   prénoms/patronymes/villes SN) sont publiables. L'auditabilité du corpus
   est donc bien meilleure que supposé.

## R8. Checklist de validation avant mise en production (votre côté)

1. Roundtrip PII synthétique : **zéro valeur réelle** capturée côté
   fournisseur (capture au mock), restaurations exactes.
2. Message **long** (> 5 000 tokens) : `inférence échouée = 0` dans les logs
   du daemon (fenêtrage v0.3.3.1).
3. Streaming : sentinelle coupée entre chunks → restaurée ; pas de sentinelle
   résiduelle dans la réponse.
4. tool_calls aller/retour : arguments JSON valides, valeurs restaurées.
5. Passe observe-only initiale exploitée (R3), faux positifs listés.
6. Vault : sauvegarde + restauration testées une fois (procédure écrite),
   TTL adapté à vos sessions.
7. Egress : aucun appel hors `127.0.0.1:8787` et l'URL du fournisseur
   (vérif réseau type `ss`/`tcpdump` une fois — Q13).

## R9. Ce que CLOISON doit vous fournir (à arbitrer côté pilote)

- **Réponses formelles Q1–Q20** (≈ 80 % répondables immédiatement depuis le
  code, les tests et les mesures de cette session) ;
- **Feuille de route** : `/v1/messages` (Q1), `/v1/responses` (Q2), doc
  observe-only (Q10), lexiques projet (Q8) ;
- **Éléments juridiques** (DPA, registre, analyse CDP — Q19) : décision
  pilote + conseil, hors périmètre technique ;
- **Session technique conjointe** : à caler dès que votre environnement
  Omarchy est prêt (votre proposition, scénario streaming + tools).

---

*Rédigé le 06/09/2026 (session phase serveur, edge v0.3.3.1 en prod).*
*Sources : `cloison/journal/E2E-MANIA-TENANT.md` §06/09, tests e2e
`cloison-proxy/tests/e2e.rs`, `ARBITRAGE-05-SENTINELLES-VS-FAKE.md` §6.*
