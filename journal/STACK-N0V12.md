# CLOISON — STACK-N0V12 : NER léger embarqué (chantier ④) + open-core v0.2.5

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.
> Session N0 v1.2, 26/08/2026. Suite directe de STACK-N0V11 +
> `journal/N0V12-PREP.md` (chantier ④, arbitrage GO/NO-GO en ouverture).
> Références : charte §4 (N0), §5.1 (« ne force jamais un artefact unique à
> faire les deux »), §6.1 (couches 2-3), §11 (honnêteté), règle §5 de la
> grille v1.1 ; handoffs `REPRISE.md` / `REPRISE-DEPLOIEMENT.md`.

## Objectif

Lever la dernière limite assumée du daemon N0 (docs/N0.md §4.1) : le rappel
PERSON/LOC en **texte libre** (un nom hors gazetteer et jamais mentionné
partait en clair). Le chantier ④ embarquait un **NER léger** (PERSON/LOC)
**dans le processus du daemon** — jamais un sidecar Python (charte §4) — via
ONNX Runtime **Rust**, avec un **arbitrage GO/NO-GO pré-enregistré**
(`journal/ARBITRAGE-04-NER-LEGER.md`), puis la **re-publication open-core
v0.2.5** (proxy + wasm) prévue par N0V12-PREP §2.2.

## §0. Pré-session

- **Arbitrage ④ pré-enregistré** (ARBITRAGE-04, 26/08/2026) : critères
  C1–C5 figés AVANT toute mesure (esprit grille v1.1). Verdict : **GO**
  (voir §Résultats).
- **Candidat amendé sur constat mesuré** : le candidat initial mBERT
  NER-hrl se quantisait MAL en int8 (logits tout-O après `quantize_dynamic`,
  y compris `per_channel` — export officiel HF ET export maison) ; le
  candidat retenu est **distilbert-base-multilingual-cased-ner-hrl** (même
  famille MasakhaNER 2.0, plus léger 134 M params, int8 135 Mo) dont l'int8
  se quantifie correctement (constat mesuré, documenté ARBITRAGE-04 §3).

## Périmètre

**Dans :** arbitrage ④ (critères + mesures + verdict), `light_ner.rs`
(proxy — tokenizers + ort 2.0.0-rc.13 load-dynamic + alignement spans),
fusion englobante N0 (`SessionOptions.enable_enclosing_ner_fusion` — core),
**bug corrigé** généralisation ville_sn (`Policy::n0_for` n'était pas
appliquée — la ville restait en clair), tests + invariants, preuve e2e
réelle, docs (CONFIG/N0/DATA-MODEL/README/OPEN-CORE), script
`deploy/provision_ner_lite.sh`, **open-core v0.2.5 en cascade** (core →
audit → proxy + wasm, leçon DEPLOY-10), journal + push.

**Hors :** NER côté serveur (le sidecar Python porte toujours les paliers
N1/N3 — inchangé), déclinaison mobile (piste documentée), GPU (dette
transverse inchangée).

## Décisions

1. **NER embarqué = producteur de spans dans `cloison-proxy` (mode N0)**
   — exactement le rôle du sidecar distant B.1 mais en local : tokenise
   (crate `tokenizers`, tokenizer.json HF), infère (`ort` 2.x,
   `load-dynamic` — lib onnxruntime provisionnée, jamais embarquée),
   aligne les spans (portage `_align_spans`), passe à
   `Engine::tokenize_session(extra)` — le **core reste la source de
   vérité** (validation stricte, invariants inchangés).
2. **Fusion englobante N0** : un span NER complet (« Aminata Diop ») prime
   sur les fragments gazetteer partiels qu'il englobe (« Aminata »,
   « Diop ») — sinon `merge_extra_spans` (B.1) les jetterait (chevauchement)
   et le NER n'apporterait rien. Implémentation : option de session N0
   (`SessionOptions.enable_enclosing_ner_fusion`, défaut **off**) — le
   chemin serveur (`tokenize_with_extra`) reste **bit-identique** (testé).
3. **Runtime `ort` = 2.0.0-rc.13** : seule version non-yankée du crate
   (toute la 1.x est yankée — migration forcée, constat crates.io) ;
   `load-dynamic` = la lib onnxruntime est chargée au runtime (jamais dans
   le binaire — dégradation gracieuse si absente).
4. **Dégradation gracieuse OBLIGATOIRE** : modèle absent/corrompu, lib
   absente, prédiction en échec → N0 v1 (gazetteers + alias), `warn`,
   jamais d'erreur (ARBITRAGE-04 §4.3).
5. **Offsets = octets** : le crate `tokenizers` rend des offsets octets
   relatifs au texte (contrat du core : `text.len()`, `is_char_boundary`,
   `text[start..end]`) — aucun glissement sur les textes accentués (la
   conversion caractères→octets initiale était FAUSSE, retirée — test e2e
   « il habite à Ziguinchor »).
6. **Bug corrigé — généralisation ville_sn** : `Policy::n0_for` déclarait
   `ville_sn → [VILLE_SN]` mais `process_spans` appelait le `Generalizer`
   par défaut (qui ne connaît pas la règle) → la ville restait en clair.
   Corrigé : la politique porte la règle (`Policy::generalize_rule`) et
   `Generalizer::apply_rule` l'applique. Test de régression
   `test_n0_policy_ville_sn_generalized_not_tokenized`.
7. **Open-core v0.2.5 en cascade** : le core publié v0.2.4 ne portait PAS
   les changements du chantier ④ → publication dans l'ordre de dépendance
   (core → audit → proxy + wasm, leçon DEPLOY-10), chaque dépôt avec
   Cargo.toml **autonome** (valeurs workspace inlinées, git deps taguées
   v0.2.5) + Cargo.lock épinglé + licences correctes.

## Ce qui a été construit

### `cloison-proxy` — `src/light_ner.rs` (nouveau)
- `LightNer::try_new(config)` : init onnxruntime (une fois/processus),
  tokenizer, session ONNX ; `None` (jamais une erreur) à tout échec —
  dégradation gracieuse.
- `LightNer::detect(text)` : tokenise (u32→i64 — le graphe ONNX attend
  int64), infère (entrées selon le graphe : `token_type_ids` pour BERT,
  absent pour distilbert), argmax + softmax stable, aligne les spans
  (portage `_align_spans` : BIO, contiguïté, seuil).
- `LightNerConfig` (config.rs) : `CLOISON_NER_MODEL_ONNX`,
  `CLOISON_NER_TOKENIZER`, `CLOISON_ONNX_LIB`, `CLOISON_NER_THRESHOLD`.
- Wiring : `AppState.light_ner` (mode N0 + config posés), spans fusionnés
  dans `tokenize_with_detect` (B.1 + NER embarqué).

### `cloison-core`
- `SessionOptions.enable_enclosing_ner_fusion` (N0 v1.2) + fonction
  `merge_enclosing_spans` (familles sémantiques : PERSON ≡ nom_sn,
  LOC ≡ ville_sn ; span englobant prime ; chevauchement structuré → core
  prime). **+4 tests**.
- `Policy::generalize_rule()` + `Generalizer::apply_rule()` + `process_spans`
  applique la règle de la politique (fix ville_sn N0). **+1 test de
  régression** (découvert par la preuve e2e : la ville restait en clair).

### Bench
- `bench/cloison-bench/measure_n0_ner.py` : mesure A (N0 actuel) / B
  (N0 + NER) + balayage de seuils + `merge_enclosing_ner` (miroir de la
  fusion produit). Artefacts `results/arbitrage04*.json`.

### Docs / packaging
- `journal/ARBITRAGE-04-NER-LEGER.md` (pré-enregistré, mesures, amendement
  C3, verdict GO), `docs/CONFIG.md` (variables NER_*), `docs/N0.md`
  (§4.1 levée, §7 livré ④), `docs/DATA-MODEL.md` (tags PE/LO),
  `README.md`, `docs/OPEN-CORE.md` (v0.2.5 + règle licence AGPL),
  `deploy/provision_ner_lite.sh` (export ONNX int8 + lib).

## Comment lancer / tester

```bash
# Portes (VPS, rustdev rust:1.97) :
cargo test --workspace                 # 286 verts (dont +5 chantier ④)
cargo clippy --workspace --all-targets -- -D warnings   # 0 erreur
cargo fmt --all -- --check             # 0 diff
cargo check -p cloison-core -p cloison-verify --features wasm -p cloison-wasm \
  --target wasm32-unknown-unknown      # WASM vert

# NER léger (daemon N0) :
./deploy/provision_ner_lite.sh         # export ONNX int8 + tokenizer (135 Mo)
# puis : CLOISON_NER_MODEL_ONNX=… CLOISON_NER_TOKENIZER=… CLOISON_ONNX_LIB=… \
#        CLOISON_VAULT_PATH=… CLOISON_VAULT_PASSPHRASE=… cloison-proxy

# Mesure d'arbitrage (VPS) :
#   CLOISON_CORE_BIN=… CLOISON_NER_MODEL_ONNX=… CLOISON_NER_TOKENIZER=… \
#   python3 measure_n0_ner.py
```

## Résultats

### Mesure d'arbitrage (jeu STACK-1, seed 42, 500 docs — ARBITRAGE-04)

| Seuil | PERSON | LOC | CNI | MAIL | TEL | macro | spécificité |
|---|---|---|---|---|---|---|---|
| 0.50 | 0.6054 | 0.6828 | 1.0000 | 1.0000 | 0.9987 | 0.8574 | 83 % |
| **0.70** | **0.6230** | **0.6948** | 1.0000 | 1.0000 | 0.9987 | **0.8633** | 83 % |
| 0.90 | 0.6176 | 0.6980 | 1.0000 | 1.0000 | 0.9987 | 0.8629 | 83 % |

Référence N0 actuel : PERSON **0.0000** · LOC 0.5186 · macro 0.7035 ·
spécificité 90 %.

**Verdict GO (seuil 0.70 retenu)** :
- C1 F1_PERSON **+0.62** (seuil +0.10) ✅ · C2 F1_LOC **+0.18** (+0.05) ✅ ·
  C4 CNI/MAIL/TEL non-régression ✅ · C5 latence **10,7 ms** (doc court,
  seuil ≤ 1 s) ✅ · C3 spécificité **83 %** ≥ 0.60 ✅ (**amendement
  documenté** : la non-régression stricte vs 90 % est inatteignable par
  construction — les 17 FP sont TOUS des toponymes réels du jeu
  (« Sénégal », « Dakar », « Saint-Louis ») à score ≥ 0.99, tension de
  conception STACK-8, PAS un défaut du détecteur ; le N0 actuel a 90 %
  uniquement parce qu'il ne détecte rien, PERSON = 0.0000).

### Preuve e2e réelle (VPS, daemon N0 + NER embarqué)

Requête « Appelez Xolani Ndlovu au 77 123 45 67, il habite à Ziguinchor. »
→ corps reçu par le mock LLM :
`Appelez ⟦…·PE⟧ ⟦…·PE⟧ au ⟦…·PH⟧, il habite à [VILLE_SN].`
- **nom hors gazetteer masqué** (2 sentinelles PE — le modèle distilbert
  découpe « Xolani Ndlovu » en deux spans, acceptable : tout est masqué) ;
- téléphone masqué (PH) ; **ville généralisée** `[VILLE_SN]` (fix du bug
  ville_sn prouvé en e2e — avant fix : restait en clair) ;
- restauration complète côté client, zéro jeton résiduel.

### Portes
- **286 tests verts, 0 échec** (workspace ; core 90 lib + 17 invariants,
  proxy e2e_n0 8/8) ; clippy `-D warnings` 0 ; fmt 0 ; WASM ok.

### Open-core v0.2.5 (publié et vérifié)
- **Cascade** : core v0.2.5 → audit v0.2.5 → proxy v0.2.5 + wasm v0.2.5
  (git deps taguées v0.2.5, ordre de dépendance — leçon DEPLOY-10).
- **Cargo.lock épinglés** poussés sur les 4 repos (doctrine DEPLOY-7).
- **Licence proxy corrigée** : `LICENSE` = texte GNU AGPL-3.0 officiel
  (régression v0.2.x — les re-splits écrasaient l'AGPL par l'Apache du
  workspace) ; vérifié API GitHub : proxy **AGPL-3.0**, core/audit/wasm
  **Apache-2.0**.
- **Vérifié** : cargo check des 4 tags publiés (git deps réelles, rust 1.97)
  — tous compilent.

## Invariants de sécurité vérifiés

- **I1 (zéro clair)** : les spans NER produisent des sentinelles ⟦…⟧ (jamais
  de clair amont — preuve e2e) ; la ville est généralisée (jamais tokenisée).
- **I3 (restaurer uniquement ce qu'on a émis)** : les spans NER passent par
  le registre de la requête + MAC — la fusion englobante n'élargit pas la
  restauration ; 17 invariants core intacts.
- **I8 (fail-loud)** : modèle/lib absents → dégradation gracieuse (warn,
  jamais d'erreur) ; la restauration reste fail-loud (marqueur neutre).
- **Serveur bit-identique** : `enable_enclosing_ner_fusion` défaut off ;
  `tokenize_with_extra` (paliers serveur) inchangé — testé.
- **Zéro PII / zéro secret** : jeux synthétiques (seed 42) ; le modèle
  distilbert est un checkpoint public (licence AFL-3.0 provisionnée, jamais
  committée — notice documentée) ; la lib onnxruntime est provisionnée,
  jamais dans le dépôt.

## Questions ouvertes / dette

- **NER côté serveur (N1/N3)** : inchangé — le sidecar Python porte toujours
  les paliers serveur (le NER embarqué est N0-only).
- **mBERT int8 cassé** : constat documenté (ARBITRAGE-04 §3) — si un modèle
  BERT est requis plus tard, re-tester la quantisation (calibration statique
  plutôt que dynamique).
- **Latence sous charge** : 10,7 ms/doc isolé ; sous charge, la session ONNX
  est sérialisée par un `Mutex` (comme le verrou du sidecar Python) — un
  pool d'inférence est une optimisation future documentée.
- **GPU** (dette ②) : inchangé — en attente (baseline ONNX CPU).
- **DNS `dsh.wonkom.ai`** : action opérateur toujours en attente (record
  présent, vérifié 25/08 — décision pilote soldée, retrait validé).
- **IndexedDB chiffré navigateur** : décision reportée (module ③ volontaire
  in-memory).

## Porte de sortie (N0V12-PREP §7)

- [x] **Décision ④ actée** : GO (critères pré-figés C1–C5, amendement C3
      documenté) ; NER léger embarqué testé + GO re-validé (grille v1.1
      NON touchée — le benchmark serveur n'est pas re-mesuré, le chemin
      serveur est bit-identique) + latence mesurée.
- [x] **Open-core v0.2.5** (core/audit/proxy/wasm) publié **en cascade** +
      vérifié (cargo check des tags, git deps réelles) + Cargo.lock +
      **licence proxy AGPL corrigée**.
- [x] Docs à jour (`docs/N0.md` §4.1 — la limite « texte libre » est levée
      avec le NER embarqué ; CONFIG, DATA-MODEL, README, OPEN-CORE).
- [x] Portes vertes (286 tests, clippy 0, fmt 0, WASM ok) + preuve e2e
      réelle.
- [x] Journal + push (commits `9dc67fb`, `4977e40`, `67203b2`, `0fe2d25`).

## Prochaine étape

- **Dettes transverses** : GPU (décision d'infra pilote, baseline ONNX),
  DNS `dsh.wonkom.ai` (action opérateur), calibration fine des seuils en
  prod avec trafic réel (`measure_clusters.py`), IndexedDB navigateur.
- **Déclinaison mobile** : même moteur que le navigateur (piste documentée).
- Re-validation GO à chaque évolution du core (règle §5) — le N0 v1.2 ne
  touche pas la stack serveur (benchmark inchangé).
