# Consolidation — PoC CLOISON N0 en environnement réel (Omarchy) — base de la prochaine session

> **Objet :** consolider tout ce qui a été testé en environnement réel sur un
> OS Linux « dernière génération » pensé pour l'agentique (Omarchy 4.0.2 /
> Arch, noyau 7.1.9), et préparer la session où nous fixons les écarts et
> re-testons. Source : dossiers `journal/REAL/` (rapports client, preuves
> brutes, scripts) + notre cycle serveur du 06/09/2026.
> **Date de consolidation :** 09/09/2026.

---

## 1. Ce qui est PROUVÉ en environnement réel (état consolidé)

| # | Preuve | Détail / source |
|---|---|---|
| P1 | Câblage réel actif | opencode v1.18.30 → CLOISON N0 **v0.3.3.1** (`127.0.0.1:8787`, service systemd user `cloison-n0`) → DeepSeek direct, clé composite `mn_omarchy.<clé>` (0600), `CLOISON_MAX_BODY_BYTES=8 MiB`, seuil NER **0,70** (choix client, cf. §3-F2) |
| P2 | Tour 1 agent | réponse **restaurée** (aucune sentinelle résiduelle) — `rapport-final §9.2` |
| P3 | Masquage aller | fournisseur ne reçoit **que des jetons** ⟦…⟧ typés (gazetteer/email/tél) |
| P4 | Coréférence + tool_calls | même valeur → même jeton ; arguments **et** résultats pseudonymisés, JSON valide |
| P5 | Streaming SSE | `reasoning_content` transmis en deltas (22 chunks vérifiés), `content` restauré — `stream-test.txt` |
| P6 | Observe-only | `=1` **et** `=true` → texte en clair côté fournisseur (E1 clos par l'expérience, conforme à notre `env_bool`) |
| P7 | Fenêtrage NER | `inférence échouée = 0` sur toutes les passes (10k/50k/100k) — critère R8 validé |
| P8 | Latence mesurée | **≈ 1,8–2,5 s / 1 000 tokens**, linéaire : 10k→13-24 s, 50k→88-129 s, **100k→193-249 s** (sandbox 4 cœurs ET install hôte `~/.cloison`) → **facteur limitant n°1 confirmé** |
| P9 | Egress | rien hors `127.0.0.1:8787` + URL fournisseur (vérifié) |

## 2. Incidents & constats consolidés (avec preuves)

| # | Constat | Gravité | Preuve |
|---|---|---|---|
| I1 | **`reasoning_content` × modèles « thinking » (DeepSeek v4) — BLOQUANT.** (a) Multi-tours : opencode/SDK ne rejoue pas `reasoning_content` → 400 `"reasoning_content in the thinking mode must be passed back to the API"` dès le tour 2 — CLOISON transmet correctement (non-stream ET SSE), **proxy non en cause** ; (b) **trou CLOISON avéré** : la restauration ne couvre pas `reasoning_content` en réponse → **sentinelle brute `⟦qeo6…·GZA⟧` visible en sortie** | 🔴 | `daemon-dbg.log` (6× 400), `opencode-dbg2.txt` l.21-22/33, `stream-test.txt` |
| I2 | **Faux positif structurel « Creuse »** (collision LO sur « heures creuses ») + ~30 jetons `·LO` sur 12k tokens de doc (lieux étrangers cités) | 🟠 | `rapport-final §8.3` |
| I3 | **Codex ≥ v0.153 a retiré `wire_api="chat"`** → plus de mode chat/completions côté Codex → `/v1/responses` (Q2) requis | 🟠 | `rapport-final §9.2` |
| I4 | E2 confirmé : défaut code seuil NER = **0,50**, commentaire `install-n0.sh` = 0,70 — divergence à trancher (leurs mesures code plaident pour 0,70) | 🟡 | `config.rs:286/617` |
| I5 | FAQ publique ambiguë (`AUDIT_MODE`) + pas de manuel N0 public | 🟡 | E1, `rapport-final §4.2` |
| I6 | E3/E4 (ledger vide, `redacted=0`) : **par conception** (reçus observe-only uniquement) — expliqués dans nos réponses formelles, plus de blocage | ⚪ | `reponses-formelles-Q1-Q20.md` |

## 3. Plan de correction — prochaine session

### F1 (🔴 HAUTE — code) : v0.3.3.2 « reasoning_content géré »

- **Aller** : `reasoning_content` devient un champ **connu et tokenisable** (comme `text`) — fini le pass-through en clair (`openai.rs`).
- **Retour** : restauration des sentinelles **dans** `reasoning_content`, non-stream (`choices[].message.reasoning_content`) **et** deltas SSE (réassemblage borné comme `content`).
- **Tests** : roundtrip reasoning non-stream + stream + sentinelle coupée dans un delta reasoning ; tests existants inchangés (26/26 lib, 12/12 e2e).
- **Cycle complet** (runbook éprouvé du 06/09) : tests locaux GNU → bundle → wonkom (portes `cargo test` + builds) → release v0.3.3.2 → rebuild edge Mania pin v0.3.3.2 → redeploy `demo-cloison` → re-sonde.
- **Acceptation** : tour N≥2 sur DeepSeek thinking = 200 ; **zéro sentinelle brute en sortie**, raisonnement inclus ; restaurations exactes.

### F2 (🟠 HAUTE — décision) : défaut du seuil NER

Trancher **0,50 vs 0,70** (code vs commentaire). Éléments : leurs mesures sur du code (faux positifs LO) plaident pour 0,70 ; nos sondes PII (noms/emails/tél) passaient à 0,50. Décision en ouverture de session, puis aligner code/commentaire/FAQ. Le client a déjà posé 0,70 manuellement.

### F3 (🟠 HAUTE — doc) : FAQ `AUDIT_MODE` + manuel N0 public

Corriger la FAQ (E1) et publier le manuel N0 (variables d'env, modes, limites, généralisation) — la matière existe dans nos réponses formelles.

### F4 (🟠 passe de Moyenne à HAUTE) : `/v1/responses`

Signal marché I3 (Codex). Décision pilote : prioriser `/v1/responses` vs `/v1/messages` vs fenêtres parallèles.

### F5 (🟡 MOYENNE) : lexique / whitelist externe

Cas concret « Creuse » (I2) à couvrir ; format de fichier lexique à définir (liste de termes à ne jamais masquer + additions de gazetteers métier).

### F6 (🟡) : `/v1/messages` — selon arbitrage client (adaptateur local accepté en attendant).

### F7 (🟡) : fenêtres d'inférence parallèles — levier n°1 de leur latence (÷ cœurs) ; PoC interne à chiffrer.

### F8 (⚪ arbitrage pilote) : DPA / registre / analyse CDP (Q19).

## 4. Plan de re-test — prochaine session

1. **R1 — Cycle v0.3.3.2** (F1) : comme au 06/09 — scripts conservés dans `SERVEUR/` (`release-v033*.sh`, `rebuild-edge-v033*.sh`, `deploy-demo-edge-v033.sh`, `sonde-pii-demo-v033.sh`, `verify-and-probe-v033.sh`).
2. **R2 — Scénario conjoint Omarchy** (leur offre) : opencode multi-tours sur DeepSeek « thinking » à travers N0 — critères F1 + P5/P6.
3. **R3 — Re-benchmark hors sandbox** (`bench-hote.sh` prêt chez eux) : chiffre de latence réel, décision d'architecture (daemon dédié, contexte réduit, F7).
4. **R4 — Passe observe-only** sur vrais flux de code : inventaire des faux positifs (dont « Creuse »), puis lexique (F5) et comparatif qualité avant/après.
5. **R5 — Vault** : exercer une fois sauvegarde/restauration (`vault.redb` + `.salt` + passphrase) + un redémarrage de daemon en session.

## 5. Décisions à arbitrer en ouverture de session

1. Seuil NER par défaut (F2) — 0,50 ou 0,70 ?
2. Périmètre v0.3.3.2 : F1 seul, ou F1 + F5 ?
3. Priorités roadmap : `/v1/responses` (F4) vs `/v1/messages` (F6) vs fenêtres parallèles (F7) ?
4. Date de la session conjointe Omarchy + scénario exact (R2).
5. Qui répond au client sur l'incident I1 (statut : proxy non en cause pour le 400 ; trou de restauration assumé et planifié en v0.3.3.2) ?

---

*Annexes consolidées dans `journal/REAL/` : rapports client (initial + final), README-POC, procédure `install-cloison-omarchy.md`, scripts bench (`gen_bench.py`, `bench-hote.sh`), preuves brutes (`stream-test.txt`, `opencode-dbg*.txt`, `daemon-dbg.log`), nos réponses formelles Q1–Q20.*
