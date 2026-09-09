# Réponses formelles Q1–Q20 — CLOISON N0 (évaluation 09/09/2026)

**Émetteur :** équipes CLOISON
**Destinataire :** équipe d'évaluation (poste Omarchy, rapport final du 09/09/2026)
**Date :** 09/09/2026
**Périmètre :** réponses fondées sur le code (dépôt `cloison`), les tests e2e et les mesures réelles du 06–09/09/2026 (edge v0.3.3.1 en prod). Chaque réponse cite sa source.

---

## 0. Écarts que vous avez relevés — vérifiés dans le code

| # | Votre constat | Verdict CLOISON (vérifié dans le code) |
|---|---|---|
| E1 | `CLOISON_AUDIT_MODE` : « `true` → masquage actif, `1` → observe-only » | **Non reproductible** : `env_bool` (`config.rs:735-747`) mappe `"1"`, `"true"`, `"yes"`, `"on"` **tous** vers `true` → observe-only (et `"0"`/`"false"`/… vers masquage actif). `handlers.rs:162` : l'état d'audit est construit ssi `audit_mode=true`, sans autre condition. Nous souhaitons un re-test conjoint (capture de ce que reçoit le mock) pour trancher l'observation ; **la FAQ reste à corriger dans tous les cas** (elle suggère l'inverse). |
| E2 | Seuil NER : commentaire d'install « défaut 0,70 » vs log `threshold=0.5` | **Confirmé** : le code est à **0,50** (`config.rs:286`, env `CLOISON_NER_THRESHOLD` défaut `0.50`, `config.rs:617`). Le commentaire de `install-n0.sh` est périmé — correction doc prévue. |
| E3 | `CLOISON_AUDIT_LEDGER_FILE` sans aucune ligne | **Par conception** : le ledger (JSONL 0600, reçus signés) n'est alimenté qu'en **mode observe-only** (`engine.rs:644-656`) ; en masquage actif, zéro ligne. Cf. Q12. |
| E4 | `redacted=0` + `publishable=false` malgré des masquages constatés | **Expliqué** : le rapport n'ingère que les **reçus observe-only** ; un test en masquage actif ne l'alimente pas (`total_requests=0`). Cf. Q11. |

---

## 1. Réponses

### Q1 — Endpoint Anthropic `/v1/messages`

**Non exposé aujourd'hui.** Les routes sont OpenAI uniquement : `/v1/chat/completions`, `/v1/completions`, `/v1/models`, `/v1/audit/report` (`routes.rs`).

- **Position sur les passerelles** : acceptable en **local** — un adaptateur (LiteLLM, claude-code-router) en `ANTHROPIC_BASE_URL`, qui traduit vers `http://127.0.0.1:8787/v1` + clé composite, ne fait sortir aucune donnée claire de la machine et ne réduit pas la garantie CLOISON (le masquage reste en aval, côté proxy). C'est le chemin recommandé pour Claude Code **aujourd'hui**.
- **Feuille de route** : `/v1/messages` = nouveau routeur + mapping Anthropic (content blocks, `tool_use`/`tool_result`) + tokenisation des seules parts texte. Travail borné ; priorisation selon votre décision d'architecture (adaptateur vs natif). **Statut : à prioriser ensemble.**

### Q2 — `/v1/responses` (OpenAI)

**Non exposé.** Même classe de travail que Q1 (nouvelle route + schéma). Pour Codex : forcer le mode chat/completions (`wire_api=chat`) ou passer par l'adaptateur. **Statut : feuille de route, priorité moyenne** (votre tableau §5 le confirme).

### Q3 — Clé amont en clair dans chaque requête

**Confirmé, par conception** : la clé composite `mn_<jeton>.<clé_amont>` porte la clé amont ; l'edge l'injecte **uniquement** dans le header `Authorization` — jamais en URL, jamais dans le corps, jamais dans les logs (invariant I1, `upstream.rs:1-6`).

- Pas de mécanisme de référencement/coffre, pas de rotation sans redémarrage, pas de multi-fournisseurs par politique **aujourd'hui**.
- En N0 loopback, l'exposition est limitée à la machine (votre point 7 est juste). **Feuille de route** : registre de clés côté edge (référence au lieu de la clé), rotation chaude, sélection de fournisseur par politique.

### Q4 — Restauration en streaming (SSE)

**Comportement confirmé, et testé** (suite e2e `e2e.rs`, 12/12) :

1. **Jeton coupé entre deux chunks** → réassemblé par buffer-and-scan borné (`max_token_len`) : test `stream_roundtrip_reassembles_split_sentinels`.
2. **Jeton inachevé à la clôture du flux** → remplacé par le **marqueur neutre** (fail-loud, jamais une demi-sentinelle) : test `stream_truncated_sentinel_redacts_at_closure`.
3. **Config** : `CLOISON_STREAM_MAX_TOKEN_LEN` (défaut **64**, plafond 256), `CLOISON_STREAM_NEUTRAL_MARKER` (défaut **`[REDACTED]`**), keep-alive (défaut **15 s**) — `config.rs:53-63`.
4. **Latence ajoutée par la couche stream : négligeable** (tampon borné). Le coût dominant reste la détection (cf. Q14).

**Nous acceptons le test conjoint** (scénario agent réel, streaming + tools) sur votre environnement Omarchy — c'est la meilleure voie pour clore Q4 et E1 en une séance.

### Q5 — Contenu non textuel

**Confirmé** : seules les parts `"text"` des contenus multimodaux sont tokenisées (`openai.rs`, `Content::Parts` — type `text` seul) ; `image_url`, `audio`, et **tout champ inconnu** traversent **intacts** dans les deux sens (`#[serde(flatten)] extra`/`rest`, invariant I6).

- **`reasoning_content`** : aujourd'hui c'est un champ inconnu → transmis **tel quel, sans tokenisation**. ⚠️ Conséquence honnête : si un modèle raisonne sur une PII masquée, son raisonnement peut la re-mentionner **en clair**. **Action produit** : traiter `reasoning_content` comme du texte tokenisable (feuille de route).

### Q6 — Sessions longues

- **TTL du vault : 7 jours** par défaut (`CLOISON_VAULT_TTL_SECS` = 604 800 s, `config.rs:77`), configurable (`CLOISON_VAULT_TTL_SECS`).
- **Coréférence** : alias intra-session + entrées du coffre ; elle tient sur toute la durée du TTL, y compris à travers les redémarrages du daemon, **tant que `vault.redb` + `.salt` + passphrase sont intacts**.
- **Redémarrage** : sans perte (le coffre est persistant et rechargé). **Rotation en cours de session** (tenant key ou sel) : **jetons antérieurs définitivement irrécupérables** (le sel participe à la dérivation des jetons) — d'où votre règle « jamais de rotation en session », que nous confirmons.
- Pour 100–200k tokens : le masquage tient (fenêtrage NER v0.3.3, `inférence échouée = 0` vérifié) ; le coût est la latence (Q14).

### Q7 — Détecteurs et politique locale

**Entités détectées en N0 (liste de ce qui est embarqué)** : gazetteer `nom_sn` (prénoms + 66 patronymes, **frontières de mot** — « Sy » ne matche plus « système »), gazetteer `ville_sn` (toponymes SN), email, téléphone (formats SN), IP, carte bancaire, dates, NER léger PERSON/LOC (seuil **0,50**), jauge quasi-id (densité, seuil 0,50).

**Configuration N0 = variables d'environnement uniquement** : `CLOISON_NER_THRESHOLD`, `CLOISON_AUDIT_K`, `CLOISON_QUASI_ID_THRESHOLD`, `CLOISON_ALIAS_EXPANSION`, … (pas de fichier de politique).

**Le `[fields] pseudonymise = [...]` de la doc** : c'est la configuration du **déploiement hébergé/N1** (politique par tenant), **pas** du daemon N0. Nous corrigeons la doc pour lever l'ambiguïté.

**Gazetteers métier** : compilés dans le binaire aujourd'hui → **feuille de route** : fichier de lexique externe (whitelist + additions par projet).

### Q8 — Faux positifs sur du code

Mécanismes existants :
1. **Frontières de mot** sur les gazetteers (05/09) — les patronymes courts ne matchent plus l'intérieur des mots (tests dédiés) ;
2. **Seuil NER** réglable (`CLOISON_NER_THRESHOLD`, défaut 0,50) ;
3. Pas de whitelist/lexique par projet **aujourd'hui** → **feuille de route** (fichier de lexique, cf. Q7).

**Méthode recommandée (la vôtre, validée)** : passe observe-only (`CLOISON_AUDIT_MODE=1`) sur de vrais flux → inventaire des faux positifs → lexique → masquage actif + comparatif avant/après.

### Q9 — Généralisation

Règles intégrées au `Generalizer` par défaut (`generalize.rs`) :
- **Date** → `YYYY-MM` (ISO `2024-03-15` et `15/03/2024` → `2024-03`) ;
- **IP** → `[IP]` (plage, préfixe) ;
- **Ville (`ville_sn`)** → **`[VILLE_SN]`**, portée par la **politique N0** (jamais de jeton : faible cardinalité, la fréquence trahirait — test `test_n0_policy_ville_sn_generalized_not_tokenized`). **Irréversible par conception** ;
- **Suppression de repli** → `[REDACTED]` ;
- Carte bancaire : règle intégrée (catégorie) — le libellé exact sera consigné dans le manuel N0 (action doc).

Niveau de configuration : règles intégrées au code ; la politique N0 (ville) est appliquée par le moteur de politique.

### Q10 — Observe-only

**Activation : `CLOISON_AUDIT_MODE=1`** (équivalents acceptés : `true`, `yes`, `on` — `env_bool`, cf. E1). Défaut (`0`/absent) = **masquage actif**. En mode 1 : détection + comptage + reçu signé par requête + rapport — **aucune transformation** du texte envoyé amont. **C'est exactement le mode non-transformant avec journalisation locale que vous cherchez.** Correction FAQ en cours (E1).

### Q11 — Rapport d'audit : sémantique exacte

Le rapport (`cloison-audit/report.rs` + `k_anonymity.rs`) est construit **uniquement à partir des reçus observe-only** :

- `total_requests` = nombre de reçus (requêtes observe-only) ;
- `masked_by_type` = compteurs bruts de détection par type (jamais exposés dans le JSON — `skip_serializing`) ;
- **`publishable`** = `true` ssi **les deux** conditions : (1) `total_requests >= k` **ET** (2) chaque compteur non nul `>= k` (`k_anonymity.rs:43-51`) ;
- **`redacted`** = les compteurs `< k` mis à **zéro** (suppression k-anonyme ; les clés de type restent visibles, leur masse ne l'est pas) — ce n'est donc pas un « nombre de masquages » mais la **projection publiable** des compteurs ;
- `k` = `CLOISON_AUDIT_K`, défaut **5** ;
- Signature **Ed25519** sur `(period_start, period_end, total_requests, redacted)` — vérifiable hors-ligne.

**Votre observation est donc conforme** : en masquage actif, aucun reçu → `total_requests=0` → `publishable=false`, `redacted=0`. **Un rapport devient publiable tel quel dès que `publishable=true`** (au moins k requêtes observe-only et aucune cellule non nulle sous k). La qualification juridique « publiable auprès d'une autorité » relève de votre analyse CDP (Q19).

### Q12 — Ledger local

- `CLOISON_AUDIT_LEDGER_FILE` = fichier **JSONL append-only 0600** de **reçus signés**, rechargé au boot ; `None` = journal en mémoire seule (`config.rs:113-116`).
- **Alimenté uniquement en mode observe-only** (`engine.rs:644-656`) → c'est pourquoi votre test (masquage actif) n'a produit **aucune ligne** (E3). Pour l'exercer : `CLOISON_AUDIT_MODE=1` + `CLOISON_AUDIT_LEDGER_FILE` posé.
- **Rejouabilité** : vérification hors-ligne avec **`cloison-verify`** (reçus + rapport signé). Le journal public `journal.wonkom.ai` est le **ledger de transparence de l'hébergé (N3)** — il ne concerne pas le ledger N0 local.

### Q13 — Sorties réseau par défaut (N0)

- Écoute : `127.0.0.1:8787` (à poser ainsi — défaut code `0.0.0.0:8787`, **poser explicitement `127.0.0.1` en N0**) ;
- **Seule sortie** : l'URL amont du fournisseur (`CLOISON_UPSTREAM_BASE_URL`). **Aucune télémétrie, aucun ping de version, aucun appel de contrôle** ;
- `CLOISON_CONTROL_URL` non posé = aucun plan de contrôle ; `CLOISON_DETECT_URL` non posé = aucun sidecar (détection embarquée seule) ;
- Garantie opérationnelle : `ss -tlnp` (écoute loopback seule) + une capture réseau ponctuelle en début de session, puis ne poser aucune variable de wiring.

### Q14 — Performance sur gros contextes

Vos mesures (poste Omarchy, sandbox 4 cœurs) : **≈ 1,8–2,2 s / 1 000 tokens**, linéaire (10k → 13–24 s ; 50k → 88–129 s ; 100k → 193–249 s). Nous les confirmons qualitativement (prod : ~26 fenêtres pour 6 700 tokens sur 2 cœurs de VPS ; roundtrip complet 101–109 s incluant l'amont).

- **Coût** : tokenisation + **inférence NER fenêtrée (séquentielle)** + détecteurs core. Le NER est le goulot.
- **Recommandations immédiates** : daemon dédié **sans bride CPU** (leçon du tenant Mania : 0,5 CPU/256 Mo sature — ne jamais reproduire), re-mesure **hors sandbox** (`bench-hote.sh` est prêt).
- **Feuille de route (priorité n°1 produit)** : **fenêtres d'inférence parallèles** — les fenêtres sont indépendantes, la parallélisation sur vos 4 cœurs divise la latence par ~4 (194 s → ~50 s à 100k). Second levier : budget de fenêtres configurable.

### Q15 — Vault & secrets

- **Keychain OS : confirmé** — `CLOISON_VAULT_KEYCHAIN_SERVICE` (+ `CLOISON_VAULT_KEYCHAIN_USER`, défaut `default`) : Windows Credential Manager / macOS Keychain / **Linux Secret Service** (`config.rs:192-199`). Repli : passphrase par env. **Fail-loud** si le coffre est actif sans aucune source de passphrase (`config.rs:487-489`).
- **Sauvegarde** : `vault.redb` + `.salt` + passphrase **ensemble** (la perte d'un seul = jetons irrécupérables, par conception et fail-loud).
- **Rotation de la tenant key sans perte de coréférence : non supportée en cours de session** (nouveau sel ⇒ nouvelle dérivation des jetons). Procédure : rotation **entre sessions uniquement**.

### Q16 — Limite de corps

- **Défaut : 1 MiB** (`CLOISON_MAX_BODY_BYTES`, `config.rs:52`), réglable par env.
- Pour des requêtes agent multi-fichiers (plusieurs Mo) : **poser explicitement** `CLOISON_MAX_BODY_BYTES` (ex. 8 MiB) ; le corps est bufferisé (coût mémoire linéaire, bien inférieur au coût NER). Aucun plafond dur côté code au-delà de la mémoire disponible.

### Q17 — Artefacts publics & licences

- **Daemon/proxy (`cloison-proxy`)** : **AGPL-3.0** (dépôt public).
- **Modèle NER** : checkpoint **public** Davlan/distilbert-base-multilingual-cased-ner-hrl, licence **AFL-3.0** (NOTICE dans le bundle) — pas un modèle propriétaire.
- **onnxruntime** : MIT.
- **Gazetteers** : listes de données (prénoms/patronymes/villes SN) — **publiables** ; nous les publierons sur demande pour auditabilité complète.
- **Client N0 autonome** = les assets de la release : binaire (AGPL) + bundle NER (AFL-3.0) + libs ort (MIT). **Impact intégration propriétaire** : l'AGPL s'applique au proxy lui-même (modifications → redistribution) ; l'usage en **processus séparé via HTTP** est la frontière classique — à valider avec votre juridique ; le bundle modèle/libs n'impose aucune obligation virale.

### Q18 — Version & cycle

- Historique récent : v0.3.0 → v0.3.1 → v0.3.2 → **v0.3.3 → v0.3.3.1** (correctifs NER fenêtrage + dégradation `response_format`).
- **Canal** : GitHub Releases (checksums SHA-256, 9 assets) + docs.wonkom.ai. Runners CI en panne depuis fin août → releases construites manuellement sur le VPS avec portes `cargo test` (jamais `cargo check` seul).
- **Compatibilité de configuration** : variables additives entre 0.3.x, pas de renommage cassant à ce jour — engagement à documenter chaque changement dans les notes de release.

### Q19 — Position contractuelle (DPA, registre, CDP)

- **Technique** : CLOISON N0 est un **outil local exécuté sur votre machine** ; aucune donnée ne transite par nos serveurs en N0.
- **Juridique** : l'analyse du régime CDP (loi n°2008-12, CEDEAO A/SA.1/01/10, Malabo ; RGPD en complément) et la qualification « responsable / sous-traitant » de votre cas d'usage relèvent de **votre conseil** ; nous fournissons la description technique précise du traitement (ce document + manuel N0) pour l'alimenter, et un **draft de DPA/registre si vous passez en production** — **statut : arbitrage pilote CLOISON**.

### Q20 — Anonymisation irréversible

**Existe : `CLOISON_REALISTIC_FAKE=1`** (v0.3.2+).

- Substitution par **faux réaliste déterministe par session**, **irréversible** (jamais la valeur réelle) pour PERSON / gazetteer `nom_sn` / téléphone / email ; les autres types retombent en sentinelles (`config.rs:101-106`).
- Conçu pour les modèles qui nettoient les sentinelles ⟦…⟧ (constat deepseek).
- **Limites exactes** : même valeur → même faux dans la session (cohérence conservée) ; pas de restauration possible (par construction) ; l'agent travaille sur des valeurs fausses mais réalistes. Conditions d'usage mesurées : matrice **ARBITRAGE-05 §6** (modèle dépouilleur, flux conversationnel dominant, aucun flux documentaire via skills, client informé). Documentation publique à compléter (action doc).

---

## 2. Feuille de route proposée (priorisée avec vous)

| # | Item | Priorité |
|---|---|---|
| 1 | **Fenêtres d'inférence parallèles** (latence ÷ cœurs) — votre facteur limitant n°1 | **Haute** |
| 2 | `/v1/messages` (Claude Code natif) | Haute (selon arbitrage adaptateur) |
| 3 | Manuel N0 public (variables, modes, limites) + corrections FAQ (`AUDIT_MODE`, seuil NER 0,5) | Haute |
| 4 | Lexique/whitelist externe (faux positifs code) | Moyenne |
| 5 | `/v1/responses` (Codex natif) | Moyenne |
| 6 | `reasoning_content` tokenisable | Moyenne |
| 7 | Registre de clés amont + rotation chaude (Q3) | Moyenne |
| 8 | DPA/registre draft + analyse CDP | Arbitrage pilote |

## 3. Engagements immédiats

1. **Session technique conjointe** : scénario agent réel (streaming + tools) sur votre Omarchy — nous fournissons les scénarios e2e de référence ; elle tranchera Q4 et E1.
2. **Corrections doc** (FAQ `AUDIT_MODE`, seuil NER, manuel N0) au prochain cycle.
3. **Gazetteers publiables** : nous les publions sur demande (Q17).

---

*Rédigé à partir de : `config.rs`, `handlers.rs`, `upstream.rs`, `engine.rs`, `generalize.rs`, `openai.rs`, `k_anonymity.rs`, `report.rs`, tests `e2e.rs` (12/12), journaux `E2E-MANIA-TENANT.md` et `ARBITRAGE-05-SENTINELLES-VS-FAKE.md`.*
