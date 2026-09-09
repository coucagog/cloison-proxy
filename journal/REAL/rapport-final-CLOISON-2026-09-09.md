# Rapport final — Évaluation CLOISON N0 : dispositif mis en place & retours d'expérience

**Émetteur :** équipe d'évaluation (poste Omarchy 4.0.2 / Arch Linux, session sandbox DSH)
**Destinataire :** équipes CLOISON (docs.wonkom.ai / github.com/coucagog)
**Date :** 9 septembre 2026
**Statut :** PoC terminé — **réponses formelles Q1–Q20 reçues et intégrées** (§8) ; câblage
réel exécuté et documenté, incident `reasoning_content` (opencode × DeepSeek) consigné (§9).
Dossier clos côté évaluation — en attente d'arbitrage utilisateur et de la session conjointe.

---

## 1. Résumé exécutif

Nous avons installé et exercé **CLOISON N0 v0.3.3.1 épinglée** (et une première passe en
`latest`), en **isolation totale** : fournisseur simulé en local, aucune donnée réelle
sortie de la machine. Le cœur de la promesse est **vérifié par l'expérience** :
pseudonymisation à l'aller (le « modèle » ne voit que des jetons ⟦…⟧ typés), restauration
au retour, coréférence intra-échange, **y compris dans les `tool_calls`** — donc
compatible avec le trafic d'un agent de codage, hors protocole (pas de `/v1/messages`).
Le **facteur limitant n°1 est la latence** de détection sur gros contextes
(≈ 2 s / 1 000 tokens sur notre machine ⇒ ~3 min à 100k tokens) : il conditionne le choix
d'architecture et doit être re-mesuré hors sandbox avant engagement.

## 2. Dispositif mis en place (inventaire)

| Artefact | Rôle | État |
|---|---|---|
| `cloison-poc/install-n0.sh` | installeur officiel relu (checksums, options) | relu, non modifié |
| `cloison-poc/.cloison/` | PoC v1 — daemon `latest` (v0.3.x) + NER | validé fonctionnellement, remplacé par v2 |
| `cloison-poc/.cloison-v2/` | PoC v2 — daemon **v0.3.3.1 épinglée** + NER | benchmark + tests |
| `~/.cloison/` | **Installation hôte standard** (v0.3.3.1) | opérationnelle (daemon testé, arrêté) |
| `cloison-poc/fake_upstream.py` | fournisseur LLM simulé (`127.0.0.1:9797`, écho OpenAI) | utilisé pour tous les tests |
| `cloison-poc/gen_bench.py` | générateur de payloads synthétiques code + PII factice | réutilisable |
| `cloison-poc/bench-hote.sh` | script de benchmark reproductible | réutilisable |
| `cloison-poc/daemon.env` / `daemon-v2.env` / `host-bench.env` | configurations test (secrets locaux, chmod 600) | à régénérer avant prod |
| `cloison-poc/install-cloison-omarchy.md` | procédure durable (service systemd user, sauvegardes) | livré |
| `cloison-poc/README-POC.md` | notes internes + corrections reçues | tenu à jour |
| `cloison-poc/rapport-CLOISON-2026-09-09.md` | rapport initial (Q1–Q20) — **déjà transmis** | transmis |
| `cloison-poc/reponse-CLOISON-2026-09-09.md` | vos recommandations R1–R9 (reçues) | appliquées ci-dessous |

## 3. Résultats de mesure

### 3.1 Validation fonctionnelle (mock local)

| Test | Résultat |
|---|---|
| PII en clair → ce que reçoit le fournisseur | **que des jetons** ⟦…⟧ (gazetteer noms, email, téléphone) |
| Réponse au client | **restaurée**, aucune sentinelle résiduelle |
| Coréférence | même valeur → même jeton dans tout l'échange |
| `tool_calls` aller/retour | arguments **et** résultats pseudonymisés, JSON valide |
| Auth composite | `Bearer mn_<jeton>.<clé>` — comparaison à temps constant |
| `CLOISON_AUDIT_MODE=1` (observe-only) | ✅ **vérifié** : le fournisseur reçoit le texte **en clair** (détection sans transformation) |
| Logs d'inférence | `inférence échouée = 0` sur toutes les passes (critère R8.2 validé) |

### 3.2 Latence de détection (contenu synthétique, masquage actif)

| Contexte | PoC v2 (sandbox) | Install hôte `~/.cloison` |
|---|---|---|
| ~10k tokens | 13–24 s | 18,5 s |
| ~50k tokens | 115–129 s | 88,9 s |
| ~100k tokens | 249 s | 193,5 s |

- Scaling **linéaire** ≈ **1,8–2,2 s / 1 000 tokens** sur cette machine (4 cœurs visibles).
- ⚠️ Mesures réalisées **sous sandbox DSH** : indicatives, à confirmer hors sandbox sur le
  poste réel (le script `bench-hote.sh` est prêt). Même en supposant un facteur 2–4 plus
  rapide hors sandbox, **100–200k tokens = dizaines de secondes à plusieurs minutes par
  requête** : c'est le point d'arbitrage n°1 (daemon dédié sans limite CPU, réduction du
  contexte injecté aux agents, ou fenêtres d'inférence parallèles côté produit).

## 4. Retours d'expérience — ce que nous avons appris

### 4.1 Corrections de votre part, validées chez nous
1. **`CLOISON_AUDIT_MODE`** : `=1` = observe-only (et non l'inverse que la FAQ suggérait).
   Vérifié par l'expérience (texte en clair côté fournisseur en mode `1`). **Demande doc** :
   la FAQ publique reste ambiguë — à corriger (cf. 4.2).
2. **Irréversibilité** : `CLOISON_REALISTIC_FAKE=1` existe (v0.3.2+) — notre Q20 devient
   « comment l'activer et quelles sont ses limites exactes ? ».
3. **Corpus** : modèle NER public (Davlan/distilbert, AFL-3.0 — NOTICE présente dans le
   bundle) et gazetteers publiable → l'auditabilité est meilleure que supposé.
4. **Version** : épingler `v0.3.3.1` (jamais `latest`) — appliqué (install hôte).

### 4.2 Écarts documentaires / comportements relevés pendant nos tests
1. **Doc publique vs binaire** : `CLOISON_AUDIT_MODE` accepte des valeurs mixtes
   (`true` → masquage actif, `1` → observe-only) sans doc claire des deux encodages.
2. **Seuil NER** : le commentaire d'install (`install-n0.sh`) annonce un défaut 0,70
   (« calibration GO ») ; le log de démarrage affiche `threshold=0.5`. Écart à clarifier
   (impact direct sur les faux positifs/négatifs).
3. **Rapport d'audit** : `/v1/audit/report` renvoyait `redacted` à 0 même quand des
   masquages étaient constatés (et `publishable: false`). Sémantique « masked_by_type »
   vs « redacted », fenêtrage et condition de publication **toujours non expliqués** (Q11).
4. **Ledger local** : `CLOISON_AUDIT_LEDGER_FILE` n'a produit aucune ligne observable en
   N0 — rôle réel et rejouabilité locale à préciser (Q12).
5. **Routeurs** : seules routes OpenAI exposées (`/v1/chat/completions`, `/v1/completions`,
   `/v1/models`, `/v1/audit/report`) — confirmé dans `src/routes.rs`. Pas de `/v1/messages`
   ni `/v1/responses`.
6. **Documentation N0** : variables d'environnement et modes non documentés publiquement
   (découverts par extraction du binaire) — un manuel N0 public serait très utile.

### 4.3 Points d'attention structurants pour notre cas d'usage (agents de codage)
1. **Latence** (mesurée, cf. 3.2) — facteur limitant n°1.
2. **Claude Code** : incompatible sans adaptateur (pas de `/v1/messages`) → décision
   d'architecture à prendre (adaptateur local vs agent OpenAI-natif en premier).
3. **Faux positifs sur du code** : à quantifier en passe observe-only sur nos vrais flux
   (noms de variables, produits, adresses de test).
4. **Sessions longues** : TTL vault 7 j, sauvegardes `vault.redb` + `.salt`, interdiction
   de rotation en cours de session — intégré à notre procédure.
5. **N0 et poste compromis** : limite assumée — l'analyse de risque doit la porter.

## 5. Questions restées ouvertes (priorisées)

| # | Question | Priorité |
|---|---|---|
| Q1 | Feuille de route `/v1/messages` (Claude Code) ? | Haute |
| Q4 | Confirmation formelle restauration SSE (découpage/neutral marker) + test conjoint | Haute |
| Q6 | TTL vault par défaut confirmé (7 j ?) ; impact rotation en session | Haute |
| Q7 | Politique locale : liste exhaustive des détecteurs, configuration, gazetteers métier | Haute |
| Q10–12 | Observe-only, sémantique rapport/ledger (écarts 4.2.3/4.2.4) | Haute |
| Q2 | `/v1/responses` (Codex natif) | Moyenne |
| Q8 | Lexiques projet / lutte faux positifs | Moyenne |
| Q19 | DPA / registre / analyse du régime CDP | Moyenne (arbitrage pilote) |
| Q20 | `CLOISON_REALISTIC_FAKE` : activation + limites exactes | Moyenne |

## 6. Demandes concrètes aux équipes CLOISON

1. **Réponses formelles** aux questions ci-dessus (la plupart répondables depuis le code,
   les tests e2e et vos mesures — cf. vos R9).
2. **Correction de la FAQ** sur `CLOISON_AUDIT_MODE` et clarification seuil NER 0,5 vs 0,7.
3. **Manuel N0 public** (variables d'env, modes, limites).
4. **Feuille de route** : `/v1/messages`, `/v1/responses`, fenêtres d'inférence parallèles,
   observe-only documenté.
5. **Session technique conjointe** : scénario agent réel (streaming + tools) — notre
   environnement Omarchy est prêt.
6. **Éléments juridiques** (DPA, registre, analyse CDP) si passage en production.

## 7. Prochaines étapes (côté évaluation)

1. Re-mesure **hors sandbox** sur le poste Omarchy (`bench-hote.sh`) — décision d'architecture.
2. Passe **observe-only** sur de vrais flux de code (liste des faux positifs) — attend des
   sessions réelles.
3. Arbitrage : adaptateur Claude Code vs agent OpenAI-natif en premier.
4. Comparatif qualité des réponses avant/après masquage.

---

*Environnement : Omarchy 4.0.2 (Arch), noyau 7.1.9, session sandbox DSH 4 cœurs, binaire
CLOISON v0.3.3.1 (checksums SHA-256 vérifiés). Tous les tests : fournisseur simulé local,
aucune donnée réelle sortie de la machine.*

---

## 8. Mise à jour (09/09, après réponses formelles Q1–Q20) — état du cycle

### 8.1 Réponses formelles reçues
Vos réponses complètes figurent dans `reponses-formelles-Q1-Q20.md` (Q1–Q20 + écarts
E1–E4 vérifiés dans votre code + feuille de route en 8 items + engagements).

### 8.2 Écart E1 — clos de notre côté
Vous souhaitiez un re-test conjoint sur `CLOISON_AUDIT_MODE`. **Nous l'avons tranché
localement** : avec `CLOISON_AUDIT_MODE=true`, le fournisseur (mock) reçoit le texte **en
clair** → `true` = observe-only, conforme à votre `env_bool` (E1). La FAQ reste à corriger ;
le point n'appelle plus de test conjoint de notre côté.

### 8.3 Travaux complémentaires réalisés
1. **Passe de détection sur notre code réel** (`passe-faux-positifs-code.md`) : aucun faux
   positif sur identifiants/chemins ✅ ; **faux positif structurel « Creuse »** (collision
   toponyme LO sur « heures creuses ») ❌ ; 30 jetons `·LO` sur 12k tokens (lieux étrangers
   cités dans la doc) ; emails/phones d'exemple masqués. → Recommandations : seuil NER plus
   haut (0,70) et whitelist/lexique projet (vos Q7/Q8).
2. **Arbitrage d'architecture proposé** (`decision-architecture-agent.md`) : démarrer par un
   agent **OpenAI-natif** (1 saut, zéro composant) ; Claude Code via adaptateur local en
   second temps ou si imposé. **En attente d'arbitrage utilisateur.**
3. **Procédure durable** (`install-cloison-omarchy.md`) mise à jour : `CLOISON_MAX_BODY_BYTES`
   à poser (défaut 1 MiB, Q16), écoute `127.0.0.1` explicite (Q13), service systemd user.

### 8.4 Ce qui reste ouvert (côté évaluation)
- Re-mesure hors sandbox (`bench-hote.sh`) et session conjointe (Q4 streaming réel) ;
- Arbitrage utilisateur : choix de l'agent (et déclaration agent par défaut Omarchy) ;
- Passe observe-only élargie sur les vrais flux des agents ;
- Éléments juridiques (Q19) — arbitrage pilote.

---

## 9. Suivi (09/09 soir) — câblage réel exécuté & incident `reasoning_content`

### 9.1 Réponses formelles — statut confirmé
Réponses Q1–Q20 reçues et intégrées (§8 + `reponses-formelles-Q1-Q20.md`). Aucune relance
externe nécessaire : l'incident ci-dessous recoupe votre réponse **Q5** (`reasoning_content`
non tokenisable/non géré — feuille de route item 6), que nous confirmons désormais comme
**bloquant pour les agents DeepSeek « thinking »**, et non plus cosmétique.

### 9.2 Câblage exécuté (validé utilisateur)
Codex puis opencode → CLOISON N0 (`127.0.0.1:8787`) → DeepSeek (`api.deepseek.com`),
clé composite `mn_omarchy.<clé>` (chmod 600), service systemd user `cloison-n0` actif,
`CLOISON_MAX_BODY_BYTES=8 MiB`, seuil NER 0,70. Config Codex neutralisée (`.bak-cloison`)
car **Codex ≥ v0.153 retire `wire_api="chat"`** (réponses API uniquement, non exposée par
CLOISON — votre Q2). opencode v1.18.30 installé + provider `cloison-deepseek`.

### 9.3 Incident — agents multi-tours × DeepSeek v4 « thinking »
- **Symptôme** : 1er tour OK (réponse restaurée ✅), tours suivants → `400 DeepSeek :
  "reasoning_content in the thinking mode must be passed back to the API"`.
- **Diagnostic (preuves locales)** : CLOISON transmet `reasoning_content` dans les deux sens
  (non-stream ET stream SSE — 22 chunks vérifiés) ; multi-tour curl **avec** replay du
  reasoning → 200 ✅ ; multi-tour sans thinking → 200 ✅. L'échec vient d'**opencode/SDK**,
  qui ne stocke/rejoue pas `reasoning_content` (champ non standard) — le proxy n'est pas en
  cause.
- **Tentative de contournement** (`thinking:{"type":"disabled"}`, accepté par DeepSeek en
  curl) : instable via opencode (sentinelle non restaurée visible en sortie, boucle) →
  abandonnée, config restaurée propre.
- **Statut** : à faire remonter à opencode (replay `reasoning_content` / body personnalisé) ;
  votre item feuille de route « reasoning_content » (Q5/roadmap 6) passe en priorité haute
  pour tout usage agent sur modèles DeepSeek/GLM « thinking ».

### 9.4 Enregistré pour la suite
- Session conjointe : inclure le scénario « agent multi-tours sur modèle thinking » ;
- Surveillance : versions opencode (fix replay reasoning), Codex (retour possible d'un mode
  chat-compat), CLOISON (`/v1/responses`, gestion reasoning_content).
