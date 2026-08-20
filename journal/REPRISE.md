# CLOISON — Document de reprise (handoff)

> Écrit le 20 août 2026 par Ridwan (agent de dev). À lire EN PREMIER par toute
> session qui reprend le projet. Complète `journal/STACK-*.md` (détails par étape).

---

## 1. Le projet en une phrase

**CLOISON** : proxy de confidentialité PII compatible OpenAI. Il pseudonymise les données
personnelles avant qu'elles n'atteignent un LLM (jetons ⟦…⟧), et restaure les vraies valeurs
dans la réponse. Le moteur descend chez le client (edge) ; le cloud n'est qu'un plan de
contrôle aveugle. Projet indépendant de mania.sn. Nom de travail CLOISON.

## 2. Où est le code

- **Dépôt** : GitHub `coucagog/cloison` (PRIVÉ), branche `main` — **12 commits** (STACK-0 → 7 + correctifs).
- **Serveur de dev** : machine hôte du VPS OVH (`wonkom.ai`). J'y accède par SSH :
  `ssh debian@172.17.0.1` (clé SSH configurée, sans mot de passe).
- **Le repo vit sur l'hôte** : `/home/debian/Cloison/cloison/` (c'est la source de vérité).
- Mon container Docker (où tourne cette session) : workspace `/root/Cloison/`, les
  **documents de référence** sont dans `/root/Cloison/reference/` (copies) et
  `/home/debian/Cloison/reference/` (originaux sur l'hôte).
- **Accès GitHub** : token stocké sur l'hôte dans `~/.git-credentials` (mode 600), pas dans
  le repo. Compte : `coucagog@gmail.com`.

## 3. Les documents à RELIRE avant tout travail (source de vérité)

1. **`/home/debian/Cloison/reference/CLOISON-NOTE-TECHNIQUE.md`** — LA charte fondatrice
   (478 lignes). Invariants de sécurité non négociables (§2), architecture (§5), protocoles
   (§7), schémas (§9), plan STACK-N (§13). **Ne pas en sortir.**
2. **`/home/debian/Cloison/reference/cloison-topologie_PII_V3.html`** — version illustrée de
   la topologie et du vocabulaire.
3. **`journal/STACK-0.md` → `STACK-7.md`** (dans le repo) — journaux détaillés de chaque
   étape : décisions, résultats, portes de sortie, dettes.
4. **`docs/SECURITY.md`** — les 12 invariants (recopiés de la charte).
5. **`docs/DEPLOY.md`, `docs/CONFIG.md`, `docs/API.md`** — comment déployer et configurer.

## 4. État d'avancement (STACK-0 → 7)

| STACK | Livrable | Tests | Statut |
|---|---|---|---|
| 0 | Monorepo, CI, docs | — | ✅ livré |
| 1 | Benchmark baseline + grille v1.1 | 32 pytest | ✅ livré |
| 2 | `cloison-core` (Rust) | 56 (39 unit + 17 invariants) | ✅ livré |
| 3 | `cloison-proxy` (Axum) | 25 | ✅ livré (3 P0 QA corrigés) |
| 4 | Mode Audit (reçus signés, k-anonymat) | 56 (34+22) | ✅ livré (3 P0 corrigés) |
| 5 | control/ledger/verify | 88 | ✅ livré (NO-GO QA résolu) |
| 6 | `cloison-detect` (Python NER) | 67 pytest | ✅ livré (GO conditionnel résolu) |
| 7 | Docker, Helm, docs, e2e | 202 Rust + 67 Python | ✅ livré |

**Total : 269 tests verts, clippy -D warnings = 0.** Dernier commit : `15cd2f1`.

**Preuves de bout en bout (STACK-7) :**
- E2E mock **12/12 PASS** : le faux LLM reçoit des sentinelles ⟦, jamais la PII ; le client
  reçoit la PII restaurée. Un proxy pass-through échouerait.
- E2E réel contre OpenRouter **8/8 PASS** : nom/téléphone/email simulés restaurés, aucun
  jeton résiduel. Le produit fonctionne contre un vrai LLM.

## 5. TÂCHES SUIVANTES (par ordre de priorité)

### ⚠️ PRIORITÉ 1 — Le GO/NO-GO final est NO-GO mais INCOMPLET (à ne pas enterrer)
Le benchmark `bench/cloison-bench/run_detect_target.py` (commité) tourne, verdict **NO-GO**,
mais ce run est **offline et tronqué** :
- Le détecteur africain est **inactif** (« chargement impossible » — modèles HF non
  téléchargés, pas de réseau GPU).
- Résultats offline : macro 0.786 (baseline 0.750), PERSON 0.613 (baseline 0.518, +0.095),
  LOC 0.613 (+0.017), CNI 0.79 (baseline 1.0), TEL 1.0, MAIL 0.91, spécificité 27%.
- **Le verdict honnête n'est pas encore rendu** : il faut rejouer AVEC les modèles africains
  réels (SERENGETI/AfroXLMR — GPU) et/ou calibrer. C'est la décision stratégique qui attend
  MLS : le fossé produit repose sur PERSON/LOC (+0.12/+0.15 exigés, ~+0.095/+0.017 obtenus
  offline).
- **Décision à trancher par MLS** : poursuivre le produit (investir GPU/modèles), réorienter
  (contribution upstream Presidio), ou abandonner. La grille v1.1 (option 1) fixe les seuils.

### PRIORITÉ 2 — Bugs de couverture encore ouverts (découverts pendant le benchmark)
- **CNI F1 0.79** (baseline 1.0) : conflit CreditCard vs CniSn sur les 13 chiffres précédés
  d'une lettre (« numéro 1078... ») — le détecteur CreditCard capture avant CniSn dans
  certains textes. À corriger (priorité au type spécifique CNI sur chevauchement).
- **Spécificité non-PII 27%** (min 60%) : le pipeline génère beaucoup de faux positifs sur
  les documents sans PII — à investiguer (quels détecteurs sur-détectent ?).
- **PERSON/LOC 0.61** : plafonné offline (sans modèles africains + sans GLiNER réel chargé ?).

### PRIORITÉ 3 — Dettes techniques consignées (dans les journaux STACK)
- `PostgresStore` : le trait Store est prêt, l'impl Postgres réelle reste à faire (STACK-5 debt).
- Journal des reçus audit **en mémoire** : perte au restart → stockage JSONL 0600 à ajouter.
- `GET /v1/audit/report?period=...` : le paramètre period est accepté mais pas filtrant.
- Modèles africains (SERENGETI E250, AfroXLMR) : non téléchargés, GPU requis.
- `[profile.release]` (strip/lto) non configuré ; `trivy-action@master` non pinné.
- E2E réel : le `invalid JSON from upstream` initial a mené à la correction de l'URL (doublon
  /v1) — vérifier que le e2e complet passe en `CLOISON_E2E_MODE=both` sur un repo frais.

### PRIORITÉ 4 — Déploiement wonkom.ai (quand le GO est tranché)
- Suivre `docs/DEPLOY.md` : docker compose (edge 8787, control 8788, detect), Caddy TLS
  (déjà présent sur l'hôte — conteneur caddy tourne), secrets via `.env` (jamais commités).
- Sous-domaines prévus : api.wonkom.ai, cp.wonkom.ai, journal.wonkom.ai, detect.wonkom.ai.

## 6. Infos pratiques pour reprendre

- **Lancer les tests** : `cd /home/debian/Cloison/cloison && source ~/.cargo/env && cargo test --workspace` (202 Rust) ; `cd services/cloison-detect && source .venv/bin/activate && CLOISON_OFFLINE=1 pytest tests/` (67).
- **E2E** : `cd /home/debian/Cloison/cloison && sudo -E bash deploy/e2e_reel.sh` (mock, sans clé) ; avec `CLOISON_E2E_MODE=real OPENROUTER_API_KEY=...` pour le réel.
- **Benchmark GO/NO-GO** : `cd bench/cloison-bench && source .venv/bin/activate && CLOISON_OFFLINE=1 python3 run_detect_target.py --offline` → `results/go_nogo_final.json`.
- **Modèles du harness** : OpenRouter (clé dans `/home/node/.dsh/.credentials.yaml`) — si
  panne de crédit OpenRouter, basculer les sous-agents sur DeepSeek (provider `deepseek-official`, modèle `deepseek-v4-pro`).
- **Identité** : cette session est Ridwan, agent personnel de MLS (DG Mania) — l'identité est
  dans le preset d'agent `ridwan` (`/home/node/.dsh/.agent-presets/ridwan/`). Ne la modifier
  que sur demande de MLS.

## 7. Règles non négociables (rappel)

- Zéro PII réelle dans le code, les tests, les logs, les reçus. Tout est synthétique.
- Zéro secret dans le repo (`.env`, clés → `~/.git-credentials`, jamais commités).
- Ne jamais éditer les presets livrés du harness (`agent-presets` du déploiement).
- Les documents de référence ne quittent pas l'espace cloisonné.
- Les invariants de la charte priment sur tout ; en cas de doute, choisir l'option qui rend
  une fuite impossible par construction, et le consigner dans le journal courant.
