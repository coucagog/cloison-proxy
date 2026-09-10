# Rapport complet d'évaluation — CLOISON N0 (cycle 09–10/09/2026)

**Émetteur :** équipe d'évaluation (poste Omarchy 4.0.2 / Arch Linux, noyau 7.1.9, session sandbox DSH)
**Destinataire :** équipes CLOISON (docs.wonkom.ai / github.com/coucagog)
**Date :** cycle 9–10 septembre 2026
**Périmètre :** validation de bout en bout de CLOISON N0 comme proxy de pseudonymisation pour
un agent de codage local (opencode/Codex) relié à DeepSeek, en environnement réel Omarchy.
**Documents liés :** `rapport-final-CLOISON-2026-09-09.md` (détaillé), `reponses-formelles-Q1-Q20.md`,
`reponse-CLOISON-2026-09-09.md` (R1–R9), `accuse-reception-S1-S14-0910.md`,
`note-incident-I1-0910.md`, `consolidation-omarchy-2026-09-09.md`,
`note-transmission-R2-omarchy.md`, `protocole-R2-omarchy.md`, `sonde-reasoning-omarchy.py`,
`passe-faux-positifs-code.md`, `decision-architecture-agent.md`.

---

## 1. Résumé exécutif

Nous avons évalué **CLOISON N0 v0.3.3.2** (épinglée, checksums vérifiés) comme couche de
pseudonymisation entre un agent de codage local et DeepSeek, sur un OS « agentique » Omarchy.
Bilan du cycle :

- **Ce qui est solide et prouvé** : pseudonymisation à l'aller (le fournisseur ne voit que des
  jetons ⟦…⟧ typés), restauration au retour quand le modèle recopie les jetons verbatim,
  coréférence, tool_calls, streaming SSE, multi-tours sans 400 (fix v0.3.3.2), audit signé,
  observe-only (E1 clos), zéro télémétrie en N0.
- **Points bloquants identifiés** : (1) latence extrême des modèles « thinking » (jusqu'à
  **165 s** par requête → timeouts amont → 502) ; (2) **fragilité de la restauration** quand le
  modèle paraphrase et retire les délimiteurs ⟦…⟧ (sonde : **10/25 PASS**) ; (3) masquage
  d'entités anodines (pays/villes) qui dégrade les réponses (Sénégal → réponse « Laos »).
- **Sans fuite de clair** dans aucun scénario testé — la promesse de confidentialité tient ;
  mais le critère « réponses exploitables » n'est pas garanti sur tous les types de tâches.

Nous soumettons **18 suggestions (S1–S18)** et proposons une **session conjointe** pour
trancher les points ouverts.

---

## 2. Contexte & objectifs

- Protéger les données transitant vers des **agents de codage locaux** (aucune PII en clair
  chez le fournisseur), tout en gardant des réponses exploitables (restauration).
- Cible : agent **opencode v1.18.30** (OpenAI-compatible) → CLOISON N0 → **DeepSeek**
  (`api.deepseek.com`, modèles `deepseek-v4-flash` / `-pro` / `-vision-exp`).
- Codex écarté en cours de route : **Codex ≥ v0.153 a supprimé `wire_api="chat"`** (Providers
  → `/v1/responses` uniquement), non exposé par CLOISON (Q2) → bascule opencode.
- Cadre juridique de référence : loi sénégalaise n°2008-12, CEDEAO A/SA.1/01/10, Malabo 2014,
  régime CDP ; RGPD en complément.

---

## 3. Environnement & méthode

- Omarchy 4.0.2 (Arch), noyau 7.1.9, session d'exécution sandbox DSH (4 cœurs visibles).
- CLOISON N0 v0.3.3.2 à `~/.cloison`, service **systemd user `cloison-n0`**, écoute
  `127.0.0.1:8787`, vault AES-256-GCM persistant, clé composite `mn_omarchy.<clé>` (0600),
  `CLOISON_MAX_BODY_BYTES=8 MiB`, seuil NER 0,70, `CLOISON_UPSTREAM_TIMEOUT_MS=300000`.
- Tests : **mock local** (fournisseur simulé — zéro octet sorti) puis **DeepSeek réel**
  (données 100 % synthétiques : noms/emails/téléphones fictifs, jamais de données réelles).
- Critères d'acceptation F1 (côté CLOISON) : tour 1 restauré sans sentinelle, tour 2
  reasoning rejoué accepté (200), tour 3 stream sans sentinelle — repris par la sonde
  `sonde-reasoning-omarchy.py`.

---

## 4. Résultats de mesure

### 4.1 Validation fonctionnelle (mock local puis réel)
| # | Test | Résultat |
|---|---|---|
| V1 | Pseudonymisation aller | fournisseur ne reçoit que des jetons ⟦…⟧ typés (GZA/EM/PH/LO) |
| V2 | Restauration retour | OK quand le modèle recopie les jetons verbatim |
| V3 | Coréférence | même valeur → même jeton intra-échange |
| V4 | Tool_calls | arguments + résultats pseudonymisés, JSON valide |
| V5 | Streaming SSE | deltas transmis, réassemblage borné, `[DONE]` |
| V6 | Audit | `CLOISON_AUDIT_MODE` ; rapport signé (k-anonyme, reçus observe-only) |
| V7 | Auth | clé composite `Bearer mn_<jeton>.<clé>` — comparaison temps constant |
| V8 | Observe-only | `=1`/`true` → texte en clair amont (E1 clos par l'expérience) |
| V9 | Multi-tours reasoning | 200 avec reasoning rejoué (fix v0.3.3.2) |
| V10 | Reasoning restauré | non-stream + SSE : **0 jeton complet résiduel** (copie verbatim) |

### 4.2 Latence de détection (masquage actif, mock, sandbox 4 cœurs)
| Contexte | Mesures |
|---|---|
| ~10k tokens | 13–24 s |
| ~50k tokens | 88–129 s |
| ~100k tokens | 193–249 s |

Scaling linéaire ≈ **2 s / 1 000 tokens** sur cette machine → à 100–200k tokens, l'aller
prend des minutes : **facteur limitant n°1** (→ S2, fenêtres parallèles).

### 4.3 Passe « faux positifs » sur du code réel (13 fichiers, ~12k tokens)
- Identifiants/chemins/fonctions : **aucun faux positif** ✅
- « Creuse » (heures **creuses**) masqué (collision toponyme) ❌ ; 30 jetons `·LO` (Pékin,
  Sénégal, France…) ; emails d'exemple masqués (bruit acceptable) ; `Dakar` → `[VILLE_SN]`.

### 4.4 Sonde reasoning (v0.3.3.2, DeepSeek réel) — **10/25 PASS**
| Bloc | Résultat |
|---|---|
| HTTP 200 (tours 1–3) | ✅ |
| Streaming + `[DONE]` | ✅ |
| Multi-tours sans 400 | ✅ |
| Restauration PII (sorties structurées) | ❌ voir §6.3 |

---

## 5. Chronologie du cycle

1. **PoC initial** (v0.3.x `latest`, mock) : V1–V8 ; rapport Q1–Q20 transmis.
2. **Réponses formelles Q1–Q20 reçues** (E1–E4 vérifiés dans leur code ; E1 **clos par notre
   expérience** : `true` = observe-only).
3. **Recommandations R1–R9 reçues** puis **accusé de réception S1–S14** (tri + feuille de
   route proposée : v0.3.3.3 / v0.3.4 / v0.4.0 / F7).
4. **Câblage réel** Codex → bloqué (`wire_api` supprimé) → **opencode → CLOISON → DeepSeek**.
5. **Incident I1** (`reasoning_content` 400 multi-tours) → **corrigé par CLOISON en
   v0.3.3.2** (release le jour même) ; rebuild propre confirmé.
6. **Incidents 502** (timeout amont sur raisonnements longs) → diagnostic (direct OK,
   réinstallation sans effet) → **`CLOISON_UPSTREAM_TIMEOUT_MS=300000`** → 200 en 165 s.
7. **Sonde complète** : fragilité de restauration en sortie structurée (§6.3).

---

## 6. Incidents & points de fragilité (détail factuel)

### 6.1 I1 — `reasoning_content` × modèles « thinking » (résolu en v0.3.3.2)
- Symptôme : tour 1 OK, tours suivants → `400 "reasoning_content must be passed back"`.
- Diagnostic : CLOISON transmet le reasoning dans les deux sens (non-stream + SSE, 22 chunks
  vérifiés) ; le 400 venait d'**opencode/SDK** qui ne rejoue pas le champ (et du mode thinking
  DeepSeek qui l'exige). Correctif CLOISON v0.3.3.2 (tokenisable/restauré) → **plus de 400**
  en multi-tours avec outils. Constat résiduel : opencode **headless** non-TTY boucle/ne se
  termine pas (à confirmer en session TTY — suspecté côté opencode).

### 6.2 Incident 502 — timeouts amont (atténué)
- Symptôme : 502 `invalid JSON from upstream` (EOF) sur certains prompts ; **DeepSeek direct =
  200** ; **réinstallation propre = identique** → cause non locale.
- Cause racine : `deepseek-v4-flash` (thinking) jusqu'à **165 s** par requête → dépassement du
  timeout amont par défaut → connexion fermée → EOF.
- Correctif appliqué chez nous : timeout amont 300 s. → S15 (défaut + retry sur EOF).

### 6.3 Fragilité de restauration (sonde 10/25) — point ouvert prioritaire
- Comportement : sur les prompts à **sortie structurée**, le modèle **retire les délimiteurs
  ⟦…⟧ et recopie l'intérieur des jetons** → la restauration par correspondance exacte échoue
  → le client reçoit des chaînes pseudo-aléatoires (intérieurs de jetons).
- Sécurité : **aucune fuite de clair** (l'intérieur reste non-révélateur) — mais le critère
  « réponses exploitables » n'est pas tenu sur ces tâches.
- Garde-fou système tenté (« recopier les ⟦…⟧ à l'identique ») : **inefficace** sur ce modèle.
- → S17 (restauration par l'intérieur du jeton / marqueurs robustes) et S18 (e2e « sortie
  structurée »).

### 6.4 Dégradation qualité par sur-masquage
- « Sénégal » masqué en `·LO` → question de géographie répondue « Laos » après un long
  raisonnement. Impact direct sur l'usage général (hors code). → S16 (classes désactivables /
  whitelist géo) et S7 (lexique externe).

---

## 7. Points validés (à préserver)

- Réactivité : correctif `reasoning_content` publié dans la journée (v0.3.3.2), tests e2e
  enrichis, FAQ corrigée, **manuel N0 public**.
- Honnêteté des niveaux (N0/poste compromis, N3 = clair visible) et des limites.
- **Aucune fuite de clair observée** dans tous nos scénarios (mock et réel).

---

## 8. Suggestions consolidées (S1–S18)

| # | Suggestion | Priorité | Statut CLOISON (si connu) |
|---|---|---|---|
| S1 | `/v1/responses` (Codex) | Haute | F4 (v0.4.0) |
| S2 | Fenêtres d'inférence parallèles | Haute | F7 |
| S3 | Comptage brut local observe-only | Moyenne | Retenu |
| S4 | Défaut d'écoute `127.0.0.1` (N0) | Haute | **Corrigé, prêt (v0.3.3.3)** |
| S5 | Guide « agents × reasoning » | Moyenne | Retenu (manuel N0) |
| S6 | `MAX_BODY_BYTES` défaut ≥ 8 MiB | Moyenne | Retenu (v0.3.3.3) |
| S7 | Lexique/whitelist externe | Moyenne | F5 (v0.3.4) |
| S8 | Référence env machine (`--help-env`) | Moyenne | Retenu (v0.3.3.3) |
| S9 | Encodages booléens documentés | Faible | Fait (FAQ Q·9) |
| S10 | Sémantique compteurs d'audit | Faible | Retenu (manuel N0) |
| S11 | Builds reproductibles + signatures | Moyenne | Retenu (runners CI) |
| S12 | Installeur : épinglage affiché | Faible | Retenu (v0.3.3.3) |
| S13 | Session conjointe « agent réel » | Haute | Oui — à dater |
| S14 | Registre clés amont / rotation | Moyenne | Roadmap N1 |
| **S15** | Timeout amont ≥ 5 min / retry EOF | **Haute (incident)** | Nouveau |
| **S16** | Classes de détecteurs désactivables + whitelist géo | **Haute (qualité)** | Nouveau |
| **S17** | Restauration par intérieur de jeton / marqueurs robustes | **Haute (sonde)** | Nouveau |
| **S18** | Cas e2e « sortie structurée » | Moyenne | Nouveau |

---

## 9. Questions & demandes restées ouvertes

1. **Feuille de route datée** CLOISON (v0.3.3.3 → v0.3.4 → v0.4.0/F4 → F7) — validation conjointe.
2. **S16/S17** : arbitrage technique sur les pistes proposées.
3. **Session conjointe (S13)** : scénario R2 A/B/C — proposer une date (de notre côté :
   environnement Omarchy prêt, sonde exécutable, `CLOISON_N0_KEY` configurée).
4. **Gazetteers publiables** (Q17, engagement confirmé).
5. **Éléments juridiques** (Q19) : DPA/registre/analyse CDP — arbitrage pilote.
6. Quirk **opencode headless** (non-TTY) : confirmer en session TTY ; sinon remonter à opencode.

---

## 10. État de notre côté (configurations en place)

- Service `cloison-n0` (systemd user) actif, v0.3.3.2, upstream DeepSeek, timeout amont 300 s.
- opencode v1.18.30, provider `cloison-deepseek` (baseURL `127.0.0.1:8787/v1`, clé composite).
- Config Codex neutralisée (`.bak-cloison`) en attendant `/v1/responses`.
- Procédure durable : `install-cloison-omarchy.md` ; benchmark : `bench-hote.sh`.

---

## 11. Références

- [docs.wonkom.ai](https://docs.wonkom.ai) (accueil, produit, install N0, API, FAQ, manuel N0)
- Dépôt public [coucagog/cloison-proxy](https://github.com/coucagog/cloison-proxy) (releases
  v0.3.3.1 → v0.3.3.2)
- [openai/codex discussion #7782](https://github.com/openai/codex/discussions/7782)
  (retrait `wire_api="chat"`)
- Fichiers de preuve locaux : `upstream_seen.jsonl`, `stream-test.txt`, `daemon-*.log`,
  `opencode-*.txt`, `sonde-reasoning-omarchy.py`, `guard-test.json`.

---

*Rapport consolidé du cycle 09–10/09/2026. Tests effectués avec des données 100 % synthétiques ;
aucune donnée réelle n'a transité hors de la machine d'évaluation.*
