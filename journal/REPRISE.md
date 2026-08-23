# CLOISON — Document de reprise (handoff)

> Écrit le 20 août 2026 par Ridwan (agent de dev), **mis à jour en fin de
> session STACK-8** (verdict GO rendu). À lire EN PREMIER par toute session qui
> reprend le projet. Complète `journal/STACK-*.md` (détails par étape).

---

## 1. Le projet en une phrase

**CLOISON** : proxy de confidentialité PII compatible OpenAI. Il pseudonymise les données
personnelles avant qu'elles n'atteignent un LLM (jetons ⟦…⟧), et restaure les vraies valeurs
dans la réponse. Le moteur descend chez le client (edge) ; le cloud n'est qu'un plan de
contrôle aveugle. Projet indépendant de mania.sn. Nom de travail CLOISON.

## 2. Où est le code

- **Dépôt** : GitHub `coucagog/cloison` (PRIVÉ), branche `main` — **22 commits** (STACK-0 → 8 + correctifs).
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
3. **`journal/STACK-0.md` → `STACK-8.md`** (dans le repo) — journaux détaillés de chaque
   étape : décisions, résultats, portes de sortie, dettes. **STACK-8 = verdict GO.**
4. **`docs/SECURITY.md`** — les invariants (I1–I12 applicatifs + I-A1–A10 audit + O1–O6 ops).
5. **`docs/DEPLOY.md`, `docs/CONFIG.md`, `docs/API.md`** — comment déployer et configurer.

## 4. État d'avancement (STACK-0 → 8)

| STACK | Livrable | Tests | Statut |
|---|---|---|---|
| 0 | Monorepo, CI, docs | — | ✅ livré |
| 1 | Benchmark baseline + grille v1.1 | 32 pytest | ✅ livré |
| 2 | `cloison-core` (Rust) | 59 (42 unit + 17 invariants) | ✅ livré |
| 3 | `cloison-proxy` (Axum) | 27 | ✅ livré (3 P0 QA corrigés) |
| 4 | Mode Audit (reçus signés, k-anonymat) | 56 (34+22) | ✅ livré (3 P0 corrigés) |
| 5 | control/ledger/verify | 88 | ✅ livré (NO-GO QA résolu) |
| 6 | `cloison-detect` (Python NER) | 70 pytest | ✅ livré (GO conditionnel résolu) |
| 7 | Docker, Helm, docs, e2e | 210 Rust + 70 Python | ✅ livré |
| 8 | **Verdict GO + correctifs + dettes** | **280 verts** | ✅ livré |

**Total : 280 tests verts, clippy -D warnings = 0.** Dernier commit : `0e0a5b4` (+ docs).

**Preuves de bout en bout (STACK-7) :**
- E2E mock **12/12 PASS** : le faux LLM reçoit des sentinelles ⟦, jamais la PII ; le client
  reçoit la PII restaurée. Un proxy pass-through échouerait.
- E2E réel contre OpenRouter **8/8 PASS** : nom/téléphone/email simulés restaurés, aucun
  jeton résiduel. Le produit fonctionne contre un vrai LLM.

## 5. TÂCHES SUIVANTES (par ordre de priorité)

### ✅ PRIORITÉ 1 — RÉSOLUE : le GO/NO-GO final est **GO** (20 août 2026, STACK-8)
Le benchmark a été rejoué **avec les modèles réels** (afroxlmr MasakhaNER, téléchargé
depuis HF — **sans GPU, CPU suffit**) après correction des bugs de couverture. Verdict
grille v1.1 (5 conditions simultanées) : **GO** — `results/go_nogo_final.json` :

| Métrique | baseline | avant-fixes | **final** | seuil |
|---|---|---|---|---|
| PERSON | 0.518 | 0.613 | **0.937** | ≥ 0.638 ✅ |
| LOC | 0.596 | 0.613 | **0.835** | ≥ 0.746 ✅ |
| CNI | 1.000 | 0.791 | **1.000** | non-régression ✅ |
| MAIL / TEL | 0.985/0.652 | 0.91/1.0 | **1.000/1.000** | — ✅ |
| macro | 0.750 | 0.786 | **0.954** | ≥ 0.850 ✅ |
| spécificité | 0.42 | 0.27 | **0.77** | ≥ 0.60 ✅ |

**Le fossé ouest-africain est prouvé.** Recommandation à MLS : **poursuivre le produit**
(le GPU n'est pas requis pour le verdict ; il réduira la latence 2-6 s/doc en prod).
Runs historiques conservés : `go_nogo_final.offline-avant-fixes.json`,
`go_nogo_final.fixes-offline-serengeti.json`, `go_nogo_final.afroxlmr-sans-ville.json`,
`go_nogo_final.afroxlmr-ville.json`, `go_nogo_final.afroxlmr-ville-consensus.json`.

### ✅ PRIORITÉ 2 — RÉSOLUE : bugs de couverture corrigés (STACK-8)
- **CNI vs CreditCard** : précédence du type spécifique (CniSn prime) dans
  `crates/cloison-core/src/detection.rs` (63/182 FN) → CNI 1.0.
- **MAIL** : regex `\p{L}` (emails accentués) → 1.0.
- **Spécificité 27% → 77%** : spacy `md` par défaut (fr_sm hallucinait), seuil GLiNER
  câblé (0.45), seuils par source, **consensus PERSON/LOC** (mono-source < 0.90 refusé ;
  `CLOISON_CONSENSUS_PERSON_LOC`, exempté `recall_only`).
- **LOC 0.61 → 0.835** : gazetteer core `ville_sn` → LOC dans le benchmark (pipeline
  fidèle) + afroxlmr + consensus.
- **Défaut produit** : NER africain = `afroxlmr` (le défaut `serengeti` pointait sur un
  LM sans tête NER, inutilisable ; `masakha` est gated 401).

### PRIORITÉ 3 — Dettes : réglées en STACK-8 (sauf PostgresStore)
- ✅ Journaux `STACK-6.md`/`STACK-7.md` intégrés au repo (étaient restés dans le staging).
- ✅ Journal des reçus audit **persisté** : JSONL append-only 0600 (`CLOISON_AUDIT_LEDGER_FILE`),
  rechargé au boot, ligne corrompue ignorée (+ invariant I-A10).
- ✅ `period` **filtrant** (hourly/daily/weekly/all) sur `GET /v1/audit/report` (+ tests).
- ✅ Modèles africains **téléchargés sur l'hôte** (afroxlmr 2,1 Go, serengeti 1,1 Go ; masakha
  gated 401) — CPU suffisant, GPU non requis pour le verdict.
- ✅ `[profile.release]` strip+lto thin+codegen-units=1 ; `trivy-action@v0.36.0` pinné.
- ✅ **`PostgresStore` implémenté** (STACK-8) : feature `pg` (sqlx), `migrations/001_init.sql`,
  `CLOISON_DATABASE_URL` au boot, IDOR par requête, tests d'intégration PostgreSQL réel 2/2
  (ignorés sans base) ; compile toujours sans la feature (hors-ligne).
- ✅ E2E mock re-vérifié sur le repo à jour (SUCCÈS — masquage amont prouvé).
- ✅ Docs à jour : README/ARCHITECTURE (STACK-7 réel), THREAT-MODEL (adversaires × N0–N3 +
  honnêteté N0), DEPLOY (volet certificats charte §12), SECURITY (invariants I9–I12 + I-A10),
  CONFIG (défauts detect + DATABASE_URL), DATA-MODEL (PostgresStore), gabarit
  « Comment lancer/tester » dans STACK-3..7.
- ⏳ Reste ouvert : `session_ref_hashed` sur `request_id` → **RÉGLÉ (DEPLOY-5 :
  hash du jeton d'accès, session réelle)** ; proxy ne consomme pas
  `/v1/control/version` → **RÉGLÉ (DEPLOY-5 : long-poll + purge)** ; wiring
  edge→detect (`CLOISON_DETECT_URL`) → **RÉGLÉ (B.1, et NER africain réparé
  DEPLOY-6)** ; image detect `CLOISON_LITE=1` → **image COMPLÈTE déployée** ;
  latence CPU 2-6 s/doc → **mesurée ~0,5 s (court) / ~1,7 s (160 mots)**,
  GPU/ONNX recommandés (DEPLOY-6).

### PRIORITÉ 4 — Déploiement wonkom.ai (le GO est tranché → poursuivre)
- **DÉPLOYÉ** (DEPLOY-1→6, VPS 144.217.81.251) : edge 8787 publié
  (`api.wonkom.ai`), control 8788 interne, detect interne (image COMPLÈTE,
  afroxlmr actif, torch 2.6.0), postgres interne, journal public
  (`journal.wonkom.ai`), Caddy TLS + sonde J-14, memwatch 0 OOM.
- **Wiring C actif** : auth par hash via contrôle, ingest automatique des
  reçus d'audit (ledger public à 3 lignes), long-poll rotation.
- **CI verte (8 jobs)** + images GHCR publiées + e2e LLM réel.
- Reste à décision MLS : publication open-core (`docs/OPEN-CORE.md` §4),
  GPU (latence ~0,5 s typique mesurée — acceptable CPU), voie ONNX (piste
  documentée DEPLOY-6).

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
