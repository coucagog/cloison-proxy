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

### B. Streaming & comportement « agent »

**Q4 — Restauration en streaming.** Pouvez-vous confirmer le comportement exact de la restauration sur flux SSE : découpage d'un jeton ⟦…⟧ entre deux chunks, rôle de `CLOISON_STREAM_NEUTRAL_MARKER` / `CLOISON_STREAM_MAX_TOKEN_LEN` / keep-alive, latence ajoutée ? Acceptez-vous un test conjoint d'un scénario agent (streaming + tools) ?

**Q5 — Contenu non textuel.** Seules les parts `text` sont tokenisées (constat dans `src/openai.rs`) : confirmez-vous que les images/audio et champs inconnus traversent **intacts** ? Qu'en est-il des champs émergents type `reasoning_content` (raisonnement étendu) ?

**Q6 — Sessions longues.** Pour un agent qui enchaîne des centaines d'appels avec des contextes de 100–200k tokens : la coréférence tient-elle sur toute la durée d'une session ? Quel est le TTL par défaut du vault (`CLOISON_VAULT_TTL_SECS`) et que se passe-t-il en cas de **redémarrage / rotation du vault en cours de session** (jetons antérieurs définitivement non restaurables ?) ?

### C. Politique de pseudonymisation

**Q7 — Détecteurs et politique locale.** Quelle est la **liste exhaustive** des entités détectées en N0 et comment configurer une politique locale (activer/désactiver un détecteur, seuil NER `CLOISON_NER_THRESHOLD`, valeur de `k`, ajout de gazetteers métier) ? La doc produit présente un `config` avec `[fields] pseudonymise = ["email","phone","name"]` : à quel niveau s'applique-t-elle (N0 local, N1, hébergé) ?

**Q8 — Faux positifs sur du code.** Nos données sont majoritairement du **code et du contenu technique** (identifiants de variables, noms de produits, adresses fictives de tests). Quels mécanismes existe-t-il pour limiter les faux positifs (whitelist, lexique par projet/tenant) et évaluer l'impact sur la qualité des réponses ?

**Q9 — Généralisation.** Listez les règles de généralisation (date→`YYYY-MM`, IP→`[IP]`, carte→`••••`, ville→`[VILLE_SN]`…) et leur niveau de configuration.

### D. Audit, transparence, « observe-only »

**Q10 — Observe-only.** La FAQ documente un mode « observe-only : rien n'est transformé, tout est journalisé ». **Comment l'activer en N0 v0.3** ? Nous n'avons identifié que `CLOISON_AUDIT_MODE` (booléen). Un mode non-transformant avec journalisation locale est-il disponible ?

**Q11 — Rapport d'audit.** Sur notre test, `/v1/audit/report` renvoyait `"publishable": false` et des compteurs `redacted` à 0 malgré des masquages constatés. Pouvez-vous préciser la sémantique (fenêtre d'ingestion, condition de publication, rôle de `CLOISON_AUDIT_K`, « masked_by_type » vs « redacted ») et **à partir de quand un rapport est publiable tel quel** auprès d'une autorité (CDP) ?

**Q12 — Ledger local.** `CLOISON_AUDIT_LEDGER_FILE` n'a produit **aucune ligne** lors de notre test : que journalise-t-il exactement (reçus signés ? événements ?) et comment le **rejouer avec `cloison-verify`** en local (le journal public `journal.wonkom.ai` concerne-t-il uniquement l'hébergé ?) ?

**Q13 — Sorties réseau par défaut.** En N0, qu'est-ce qui quitte réellement la machine par défaut (aucun reçu ? compteurs ? télémétrie de version ?) et comment le garantir opérationnellement (`CLOISON_CONTROL_URL` / `CLOISON_DETECT_URL` non posés) ?

### E. Déploiement, performance, sécurité du poste

**Q14 — Performance sur gros contextes.** Coût CPU/temps de la détection (NER int8) par requête et latence ajoutée selon la taille (10k / 100k tokens) ? Recommandations d'exécution (daemon dédié, threads) pour un service user sur poste Omarchy/Arch ?

**Q15 — Vault & secrets.** L'intégration keychain Linux (Secret Service) est-elle confirmée ? Procédure de sauvegarde/restauration du vault et du sel de session (perte ⇒ jetons irrécupérables ?), et de **rotation de la tenant key sans perte** de coréférence ?

**Q16 — Limite de corps.** Valeur par défaut et maximum conseillé de `CLOISON_MAX_BODY_BYTES` pour des requêtes agent multi-fichiers (payloads de plusieurs Mo) ?

### F. Open-core, licence, feuille de route

**Q17 — Artefacts publics.** Le dépôt public (`cloison-proxy`) est en AGPL-3.0 ; le monorepo et le corpus (gazetteers, jeux d'évaluation) sont privés/propriétaires. Quels artefacts serviront un client **N0 autonome** (vérificateur WASM local, `cloison-core`, `cloison-verify`…) et quelles sont leurs licences respectives ? Quel est l'impact d'une intégration du proxy dans une stack interne propriétaire ?

**Q18 — Version & cycle.** Version testée : `v0.3.x`. Rythme de release, canal d'annonce, politique de compatibilité de configuration entre versions ?

### G. Cadre & conformité

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
