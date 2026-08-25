# CLOISON — DEPLOY-11 : Premier client N3 + calibration (chantier ②)

> Journal de déploiement — chantier ② de la session (ordre pilote : ① packaging
> N0 → ② premier client N3 + calibration → ③ décisions pilote). Session du
> 25/08/2026. Références : charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§7.4
> reçus/journal, §9.2, §11 honnêteté), `journal/STACK-N0V13.md` (①), handoffs.

## Objectif

1. **Premier client N3** : onboarding d'un tenant réel (simulateur de trafic
   représentatif — pas de client réel disponible) : tenant + jeton `mn_`,
   clé composite, requêtes à travers l'edge déployé.
2. **Calibration** des seuils sur la stack actuelle (`measure_clusters.py`,
   modèles réels afroxlmr — procédure documentée).
3. **Ledger alimenté** (seq 4+, objectif largement dépassé : seq 12) et
   **rapport k-anonyme vérifié** (signé, cellules < k redactées, publiable).

## Ce qui a été fait

### 1. Onboarding (N3 commercial, outillage DEPLOY-9)

- `cloison-cli` rebuildé sur le VPS (rustdev) et prouvé contre le contrôle
  déployé : `ledger root` (seq 2).
- Tenant **`client-demo`** provisionné (plan pro, actif) + jeton `mn_`
  (46 car., base64url) — clair affiché une fois, stocké 0600, jamais loggé.
- **Limitation découverte** : l'edge vérifie les jetons contre **un seul
  tenant** (`CLOISON_TENANT_ID`, défaut `default`) — la résolution du tenant
  par requête (charte §7.2, header) n'est **pas implémentée**. Le jeton
  client-demo (tenant client-demo) est rejeté par l'edge → le jeton de
  simulation a été émis dans le tenant `default` (documenté, dette produit).

### 2. Simulateur de trafic représentatif (`deploy/simulate_client.py`)

- Documents 100 % synthétiques du générateur STACK-1 (seed 42, 0 PII réelle)
  envoyés à `api.wonkom.ai` via la clé composite (mode audit puis masquage).
- **484 requêtes** au total : 30 + 150 + 300 (concurrence 4) — 0 erreur,
  0 sentinelle résiduelle.
- Latence : médiane ~1.4 s, p95 ~2.4 s (4 concurrents, LLM réel).

### 3. Pipeline audit → transparence PROUVÉ (le chaînon de la promesse)

- Fenêtre audit (`CLOISON_AUDIT_MODE=1`, .env + recreate edge) → reçus signés
  → ingest automatique (60 s) → contresignature contrôle → **ledger public :
  3 → 13 lignes (genèse + seq 1..12)** — appends continus (seq 3→12).
- **Rapport k-anonyme vérifié** :
  - d'abord `publishable=false` avec **DriverLicense brute = 1 < k=5** →
    correctement redactée à 0 (la redaction < k est RÉELLEMENT prouvée) ;
  - après 484 requêtes : **`publishable=true`** — tous compteurs ≥ k
    (CniSn 242, Email 443, PhoneSn 531, Gazetteer nom 227 / ville 281,
    DriverLicense 7, Matricule 6), signature Ed25519 présente,
    `aggregated` jamais sérialisé.
- **Vérification publique de la chaîne** : `cloison-cli ledger check` sur
  `journal.wonkom.ai/ledger.jsonl` + `control_pubkey.hex` →
  **`ok=true, 13 entrées, head_seq=12`** — la promesse « preuve sans texte »
  est démontrable par n'importe qui (charte §7.4).
- Edge **remis en mode masquage** (`CLOISON_AUDIT_MODE=0`) — vérifié
  (401 sans auth, `/v1/audit/report` → 404).

### 4. Calibration exécutée (`measure_clusters.py`, afroxlmr réel, torch 2.6.0)

- **TP : 1218** (identique DEPLOY-6/10) ; TP mono-source ≥ 0.9 : 1.
- **FP : 46, tous multi-sources** (toponymes réels du jeu non-PII — tension de
  conception STACK-8) ; **0 FP mono-source** → le consensus
  (refus mono-source < 0.90) tient sur la stack actuelle.
- Seuils calibrés confirmés : GLiNER 0.45, african 0.50, consensus PERSON/LOC.

### 5. Découvertes opérationnelles (dette documentée)

- **Image edge périmée** : `docker compose up -d` (sans `--build`) réutilise
  l'image locale (24/08) → le déploiement dérive de main. Découvert en
  comparant le comportement du rapport au code ; rebuild requis (fait).
  → Ajouter `pull_policy`/`--build` systématique à la doc DEPLOY.
- **Auth edge mono-tenant** (voir §1) : dette produit — résolution du tenant
  par requête à implémenter pour un vrai multi-tenant.
- **CLI flat subcommands** (`token-issue`, pas `token issue`) : les docs
  (ONBOARDING.md) utilisent la forme imbriquée — à corriger (dette doc).

## Invariants de sécurité vérifiés

- **Zéro PII réelle** : 484 requêtes 100 % synthétiques (seed 42) ; le
  simulateur ne contient aucun nom réel.
- **Zéro secret exposé** : jeton `mn_` affiché une seule fois (émission),
  stocké 0600 ; clé OpenRouter transférée par scp 0600, jamais affichée.
- **I9 (preuve sans texte)** : ledger = compteurs k-anonymes contresignés,
  jamais de texte ; rapport sans `aggregated`, signature vérifiable.
- **K-anonymat réel** : cellule DriverLicense 1 → 0 dans le rapport (masquage
  prouvé en conditions réelles), `publishable` honnête.
- **Masquage restauré** : edge remis en `AUDIT_MODE=0` (mode pseudonymisant).

## Porte de sortie (chantier ②)

- [x] Onboarding N3 démontré (tenant + jeton + verify par hash + clé composite).
- [x] Simulateur de trafic représentatif : 484 requêtes synthétiques, 0 erreur.
- [x] Ledger alimenté : **3 → 13 lignes (seq 12)** — pipeline automatique prouvé.
- [x] Rapport k-anonyme : redaction < k prouvée, puis **publiable** (tous ≥ k),
      signé, vérifié.
- [x] Chaîne vérifiable publiquement (`ok=true`, 13 entrées).
- [x] Calibration : 1218 TP / 0 FP mono-source — consensus tient.
- [x] Edge remis en mode masquage ; latence sous charge mesurée.

## Dette / suite

- Résolution du tenant par requête (auth edge multi-tenant) : à implémenter
  pour servir plusieurs clients distincts (charte §7.2).
- Image edge périmée si `up -d` sans `--build` : ajouter la doctrine à
  `docs/DEPLOY.md`.
- Calibration fine avec trafic réel : procédure prête (`measure_clusters.py`),
  à exécuter dès qu'un client réel arrive.
- Docs CLI (`token-issue` plat vs `token issue`) : à corriger.
- **Chantier ③** (décisions pilote) : GPU, DNS dsh (action opérateur),
  IndexedDB, formats passeport/permis, mobile — voir `journal/STACK-N0V13.md`.
