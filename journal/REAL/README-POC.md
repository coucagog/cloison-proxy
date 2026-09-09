# PoC CLOISON N0 — résultats (2026-09-09)

> **Mise à jour 09/09 après réponse CLOISON** (`reponse-CLOISON-2026-09-09.md`, R1–R9) :
> voir la section « Corrections reçues de CLOISON » en fin de fichier.

PoC réalisé **entièrement en local** (sandbox du workspace `/home/mls/MesProjets/cloison-poc`) :
aucun octet de donnée n'est sorti de la machine (fournisseur simulé sur `127.0.0.1:9797`).

## Validation obtenue ✅

| Test | Résultat |
|---|---|
| Installation N0 (binaire + NER ONNX + onnxruntime, checksums SHA-256) | OK — `~/.cloison` simulé dans `.cloison/` |
| Auth par clé composite `mn_<jeton>.<clé_amont>` | OK (jeton attendu = `mn_…` avec préfixe) |
| Pseudonymisation aller | « Aminata Diop / user@example.com / +221 77 123 45 67 » → le fournisseur n'a reçu **que des jetons** `⟦…⟧` typés (GZA = gazetteer noms, EM = email, PH = téléphone) |
| Restauration retour | Le client reçoit la réponse **avec les vraies valeurs**, aucune sentinelle résiduelle |
| Coréférence | Même valeur → même jeton dans tout l'échange (arguments d'outil + résultats) |
| Appels d'outils (`tool_calls`) | Arguments **et** résultats pseudonymisés, JSON toujours valide |
| Streaming + tools | Supportés par le code (`src/stream.rs`, types `tools`/`tool_calls` dans `src/openai.rs`) |
| Audit | `CLOISON_AUDIT_MODE=true` → `GET /v1/audit/report` : rapport signé (compteurs par type, k-anonymat) |

Détecteurs actifs par défaut (N0) : `Email`, `PhoneSn` (formats sénégalais), `Gazetteer(nom_sn)`, + NER `PERSON/LOC` (modèle embarqué).

## Configuration du daemon (PoC)

Fichier : `.cloison/daemon.env` — variables selon [install-n0](https://docs.wonkom.ai/install-n0.html) :

```
CLOISON_ROLE=edge
CLOISON_LISTEN_ADDR=127.0.0.1:8787
CLOISON_UPSTREAM_BASE_URL=<fournisseur réel, ex. https://openrouter.ai/api/v1>
CLOISON_VAULT_PATH=…/vault.redb          # coffre AES-256-GCM
CLOISON_VAULT_PASSPHRASE=…               # fail-loud si absente
CLOISON_EXPECTED_ACCESS_TOKEN=mn_<jeton>
CLOISON_TENANT_KEY_HEX=<64 hex>          # générée par l'installeur
CLOISON_NER_MODEL_ONNX / TOKENIZER / ONNX_LIB
CLOISON_AUDIT_MODE=true                  # rapport /v1/audit/report
```

⚠️ **Secrets PoC** : passphrase/clé de test dans `.cloison/daemon.env` (ignoré par git) — à régénérer
pour une vraie install (passphrase via keychain OS recommandé : `CLOISON_VAULT_KEYCHAIN_SERVICE`).

## Point bloquant pour l'étape (4) — agent cible = Claude Code

- CLOISON expose **uniquement des routes OpenAI** (`/v1/chat/completions`, `/v1/completions`,
  `/v1/models`, `/v1/audit/report`) — vérifié dans `src/routes.rs`.
- Claude Code parle le protocole **Anthropic** (`/v1/messages`) via `ANTHROPIC_BASE_URL`.
  → **Pas de branchement direct possible.**

### Options pour brancher Claude Code

1. **Adaptateur local Anthropic → OpenAI** devant CLOISON (ex. `claude-shadow`, claude-code-router,
   LiteLLM) : Claude → adaptateur (`localhost`, protocole Anthropic) → CLOISON N0 (pseudonymise,
   OpenAI) → fournisseur réel. Le clair circule uniquement en localhost. À évaluer : fiabilité
   streaming + tool_calls à travers les 2 sauts.
2. **Attendre/demander à CLOISON** un support `/v1/messages` (non présent dans l'open-core v0.3).
3. **Reconsidérer la cible** : un agent OpenAI-natif (Codex `wire_api=chat`, opencode custom
   provider) se branche **directement** sur CLOISON, zéro composant additionnel.

## Prochaines étapes possibles (à décider)

- Installer N0 « pour de vrai » sur l'hôte (`~/.cloison`, service systemd user ou lancement Omarchy).
- Activer le mode « observe-only » de la doc (l'équivalent N0 ici = audit + journal ; vérifier si un
  mode non-transformant existe — pas trouvé dans les env du binaire v0.3).
- Définir la politique de champs si besoin d'aller au-delà des détecteurs par défaut.

## Corrections reçues de CLOISON (09/09 — validées ou à valider)

| Sujet | Dire de CLOISON | État local |
|---|---|---|
| `CLOISON_AUDIT_MODE` | `=0`/absent → masquage actif ; `=1` → **observe-only** (détecte sans transformer) | ✅ **Vérifié par l'expérience** : `=1` → le fournisseur reçoit le texte EN CLAIR (non transformé) ; `=true` → masquage actif |
| Version | Toujours épingler `install-n0.sh --version v0.3.3.1` (jamais `latest` ; bundle NER `latest` incohérent sur Linux, historique documenté) | ⏳ Tag v0.3.3.1 confirmé existant ; daemon local encore en `latest` → **à réinstaller épinglé** |
| v0.3.3.1 | Corrige le bug NER `512×N` (gros prompts système tuaient la détection) + dégradation `response_format` (retry sans `response_format`) | ⏳ À re-tester après pin |
| Vault | Persistant, TTL défaut 7 j (`ttl_s=604800`) ; redémarrage daemon OK ; perte possible si vault/passphrase perdus ou tenant key tournée en cours de session | ⏳ À intégrer à la procédure |
| Performance | ~26 fenêtres ONNX/6,7k tokens (2 cœurs VPS) ⇒ à 100k tokens ~400 fenêtres ⇒ **dizaines de secondes par requête** = facteur limitant n°1 | ⏳ **Benchmark local à faire** (Omarchy) |
| Irréversibilité (Q20) | Existe : `CLOISON_REALISTIC_FAKE=1` (v0.3.2+) = substitution réaliste irréversible par session | ⏳ À tester |
| NER public (point 5) | Checkpoint Davlan/distilbert, licence AFL-3.0 (NOTICE dans le bundle) ; gazetteers publiable | ✅ NOTICE-AFL-3.0.txt présent dans `.cloison/ner/` |
| Streaming + tools | Restauration SSE testée (sentinelle coupée → réassemblage ; tronquée → marqueur neutre fail-loud) ; tool_calls streamés restaurés | ⏳ Scénario conjoint proposé par CLOISON |
| /v1/messages + /v1/responses | Non exposés (feuille de route) — Claude Code ⇒ adaptateur local (LiteLLM / claude-code-router) ; commencer par un agent OpenAI-natif | ⏳ Décision d'architecture |

### Check-list pré-production (R8 — notre côté, à exécuter au PoC v2)
1. Roundtrip PII synthétique : zéro valeur réelle capturée côté fournisseur.
2. Message long (> 5 000 tokens) : `inférence échouée = 0` (fenêtrage v0.3.3.1).
3. Streaming : sentinelle coupée entre chunks → restaurée, aucune résiduelle.
4. tool_calls aller/retour : JSON valide, valeurs restaurées.
5. Passe observe-only initiale exploitée, faux positifs listés.
6. Vault : sauvegarde/restauration testées, TTL adapté.
7. Egress : rien hors `127.0.0.1:8787` + URL fournisseur (vérif `ss`/`tcpdump`).

## Benchmark v0.3.3.1 — latence de détection vs taille de contexte (09/09)

Environnement : **sandbox bwrap 4 cœurs** (mesure indicative — CPU sandbox susceptible
d'être bridé ; à revalider sur le poste Omarchy réel). Masquage actif, faux fournisseur
local, contenu synthétique code + PII factice (aucune donnée réelle).

| Taille | Run 1 | Run 2 |
|---|---|---|
| ~10k tokens | 24,5 s | 13,0 s |
| ~50k tokens | 115,1 s | 128,6 s |
| ~100k tokens | 249,5 s | — (timeout 600 s) |

- Scaling **linéaire** : ~2,4–2,5 s / 1 000 tokens dans cet environnement.
- `inférence échouée = 0` dans les logs (fenêtrage OK — critère R8 validé) ; NER chargé
  (9 labels, threshold 0,5 ici).
- **Conclusion** : à 100k tokens, l'aller prend **plusieurs minutes** ici — même en
  divisant par 4 sur du matériel réel, on reste à **des dizaines de secondes par
  requête**, comme l'annonçait CLOISON (R5). → Facteur limitant n°1 confirmé pour des
  agents à contexte 100–200k : à arbitrer (daemon dédié sans limite CPU, fenêtres
  parallèles en attente côté produit, ou contexte d'agent réduit).

### À revalider sur le poste réel (Omarchy)
- Benchmark identique hors sandbox (nproc réel, cgroup non bridé).
- Test message long > 5 000 tokens avec prompts système volumineux (bug 512×N corrigé
  en v0.3.3.1) : `inférence échouée = 0`.

## Réponses formelles Q1–Q20 reçues (09/09 — `reponses-formelles-Q1-Q20.md`)

### Écarts E1–E4 : verdict
- **E1** `AUDIT_MODE` : CLOISON affirme `1`/`true`/`yes`/`on` → observe-only (env_bool). ✅
  **Vérifié chez nous** : `true` → texte en clair côté fournisseur (test du 09/09) → E1 clos.
- **E2** seuil NER : confirmé à **0,50** (notre log l'affichait) — commentaire install périmé, doc à corriger.
- **E3** ledger : **par conception**, alimenté seulement en observe-only.
- **E4** rapport : construit **uniquement sur les reçus observe-only** (`total_requests`=0 en masquage actif ⇒ `redacted=0`, `publishable=false`). `publishable=true` ssi ≥ k requêtes ET aucune cellule non nulle < k.

### Réponses clés
- **Q1/Q2** : `/v1/messages` et `/v1/responses` non exposés — feuille de route (adaptateur recommandé pour Claude Code aujourd'hui).
- **Q4** : streaming testé (suite e2e 12/12) : sentinelle coupée → réassemblée ; tronquée → `[REDACTED]` fail-loud ; `STREAM_MAX_TOKEN_LEN`=64, keep-alive 15 s.
- **Q5** : ⚠️ `reasoning_content` NON tokenisé (champ inconnu) — risque si le modèle re-mentionne une PII en clair dans son raisonnement.
- **Q6** : TTL vault 7 j (604 800 s), coréférence intra-TTL, redémarrage sans perte, rotation en session = irrécupérable.
- **Q7** : détecteurs N0 : gazetteers `nom_sn`/`ville_sn` (frontières de mot), email, tel SN, IP, carte, dates, NER PERSON/LOC (0,50), jauge quasi-id. Config = **env uniquement** (pas de fichier de politique en N0 ; `[fields]` = hébergé/N1).
- **Q9** : généralisation : dates→`YYYY-MM`, IP→`[IP]`, **ville→`[VILLE_SN]` irréversible par conception**, repli `[REDACTED]`.
- **Q13** : ⚠️ défaut d'écoute code = `0.0.0.0:8787` → **poser explicitement `127.0.0.1`** (nous l'avions fait ✅). Aucune télémétrie en N0.
- **Q16** : ⚠️ `CLOISON_MAX_BODY_BYTES` défaut **1 MiB** → **poser plus (ex. 8 MiB)** pour requêtes agent multi-fichiers.
- **Q17** : proxy AGPL-3.0, NER AFL-3.0 (Davlan/distilbert, public), onnxruntime MIT, gazetteers publiables sur demande.
- **Q20** : `CLOISON_REALISTIC_FAKE=1` = faux réaliste irréversible par session (PERSON/nom_sn/tel/email) — pour modèles qui dépouillent les sentinelles.

### Feuille de route CLOISON (priorisée)
1. Fenêtres d'inférence **parallèles** (latence ÷ cœurs — notre facteur n°1) — Haute
2. `/v1/messages` (Claude Code natif) — Haute (selon arbitrage adaptateur)
3. Manuel N0 + corrections FAQ — Haute
4. Lexique/whitelist externe — Moyenne ; 5. `/v1/responses` — Moyenne ; 6. `reasoning_content` — Moyenne ; 7. Registre clés amont — Moyenne ; 8. DPA/registre — arbitrage pilote

### Reste ouvert / décisions
- Session conjointe (streaming + tools réels) : E1 clos chez nous ; Q4 à confirmer en réel.
- Décision : adaptateur Claude Code vs agent OpenAI-natif en premier.
- Re-mesure hors sandbox ; passe observe-only sur vrais flux ; `MAX_BODY_BYTES` à poser.

## État final (09/09 soir)
- **Réponses formelles Q1–Q20 reçues** → intégrées (`reponses-formelles-Q1-Q20.md`,
  rapport-final §8, E1 clos).
- **Câblage réel** Codex→(abandonné, wire_api chat supprimé)→**opencode → CLOISON N0 →
  DeepSeek** : service `cloison-n0` actif, configs posées, clé composite en place.
- **Incident documenté** : multi-tours agents × DeepSeek « thinking » → 400 (opencode ne
  rejoue pas `reasoning_content`) — rapport-final §9. CLOISON non en cause (transmission
  prouvée). À suivre : opencode/Codex/CLOISON (roadmap reasoning_content, `/v1/responses`).
