# CLOISON — STACK-N0V11 : Alias intra-session + jauge quasi-id in-core

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.
> Session N0 v1.1, chantier ① de `journal/N0V11-PREP.md` (recommandé en
> premier : gain produit immédiat, « le plus léger possible » — moteur Rust
> seul). Suite directe de STACK-N0.
> Références : charte §6.1 (couches 4 et 6), §11 (quasi-identifiants),
> `N0V11-PREP.md` §4 (décisions), sidecar `services/cloison-detect/src/
> alias.py` + `quasi_id.py` (référence de règles — ZÉRO réécriture de logique).

## Objectif

Livrer en **in-core (Rust seul, daemon N0)** l'équivalent des deux couches du
sidecar qui manquaient au moteur léger :
1. **Alias intra-session (R1–R7)** : une mention canonique établie dans la
   conversation masque ses formes dérivées (prénom seul, titre + nom, nom
   seul hors noms communs, diminutifs, raccourcis de lieux, variantes
   casse/diacritiques) — **jamais les pronoms** (charte §6.1 couche 4).
2. **Jauge de quasi-identifiants** : densité fenêtrée des catégories
   âge + acte + date + lieu — **signal, jamais de résolution** (charte
   §6.1 couche 6, §11).

## Périmètre

**Dans :** modules `cloison-core` `alias.rs` + `quasi_id.rs` (portage fidèle
des règles du sidecar), `Engine::tokenize_session` (intégration), options de
session séparées de la `Policy` (serveur bit-identique), wiring proxy (mode
N0 : mention store par daemon, drapeau jauge → compteur + log), tests
unitaires + invariants + e2e N0, docs (N0.md, CONFIG.md, README), journal.

**Hors :** `cloison-wasm` navigateur (③), NER léger embarqué ONNX (④ — à
arbitrer), alias côté serveur (les paliers serveur restent portés par le
sidecar Python). Le chantier ② (keychain OS) a été **livré dans cette
session** (voir « Chantier ② »).

## Décisions (N0V11-PREP §4 tranchées en ouverture)

1. **Ordre des chantiers : ① en premier** (reco N0V11-PREP §2) — gain produit
   immédiat, déterministe, léger. ②③④ restent documentés.
2. **État d'alias : intra-session, en mémoire du daemon** (`AppState.session`,
   `tokio::sync::Mutex`) — **pas** dans le coffre (le coffre reste la source
   des *valeurs* ; l'alias est un état conversationnel éphémère ; « le plus
   léger possible »). Borne documentaire 200 mentions (FIFO, miroir de
   `session_mentions_max` du sidecar). La restauration reste **bornée au
   registre de la requête** (I3 inchangé — l'alias ne l'élargit pas).
3. **Jauge : formule identique au sidecar** (fenêtre 160 / pas 40 /
   bonus max 0.20, `flagged = score > seuil` strict, seuil défaut 0.5,
   **opt-in** `CLOISON_QUASI_ID_GAUGE=1`). Sortie : compteur `quasi_id_flags`
   + log — **jamais de texte**.
4. **Options de session séparées de la `Policy`** (`SessionOptions`) :
   `policy_hash` (reçus d'audit) et le comportement serveur restent
   **bit-identiques** (alias/jauge actifs en mode N0 uniquement — le serveur
   ne consulte jamais la session).
5. **Mentions canoniques** : spans PERSON/LOC uniquement (gazetteers
   `nom_sn`/`ville_sn` inclus — jamais MAIL/TEL/CNI/…), clé = texte exact
   re-tranché, `seen_count` incrémenté (boost d'alias borné, plafond
   `max_score` et canonique).

## Ce qui a été construit

### `cloison-core` — `src/alias.rs` (nouveau)
- `normalize_text` (NFKD + suppression des marques + lowercase — R7) et
  `insensitive_pattern` (classes accent-insensibles, miroir exact du sidecar).
- `AliasConfig` (défauts = sidecar : titres, diminutifs `Momo→Mamadou`,
  raccourcis `Ouaga→Ouagadougou`, noms communs, pronoms, `max_derived_forms
  = 8`, R4 off, `score_cap 0.85`, `max_score 0.95`, canonique par défaut
  0.80).
- `CanonicalMention` + `SessionContext` (upsert avec `seen_count`, borne
  FIFO, `clear`).
- `AliasExpander` : `derive` (R1–R7, trié, borné) + `expand` (index
  normalisé, matching frontière-mot **sans lookaround** — la crate `regex`
  n'en a pas : vérification manuelle des frontières `[\p{L}\p{N}_-]`, deux
  occurrences adjacentes trouvées, équivalent exact de `(?<![\w-])…(?![\w-])`),
  fusion/dédup `_merge_alias` (couvert → ignoré ; englobe strictement →
  remplacement ; chevauchement partiel → l'existant fait foi).
- **Jamais les pronoms** : bloqués à la dérivation ET au matching (tests).

### `cloison-core` — `src/quasi_id.rs` (nouveau)
- `QuasiIdCategory` (Age/Act/Date/Loc, liste fermée), `category_for`
  (Date → date ; Location/`ville_sn` → loc — PERSON/ORG/ID exclus).
- `GaugeConfig` (160/40/0.20), `QuasiIdReport` (score, flagged, signals),
  `QuasiIdGauge` (regex internes âge/acte/date + intervalles spans, fenêtres
  glissantes, bonus > 4 mentions, `score > seuil` strict).

### `cloison-core` — `src/engine.rs`
- `TokenizeResult.quasi_id` (Option<QuasiIdReport>) ; `SessionOptions`
  (enable_alias_expansion / enable_quasiid_gauge / quasiid_threshold).
- `Engine::tokenize_session(text, policy, request_id, extra, session,
  options)` : détection → spans extra validés (`merge_extra_spans` extrait
  de `tokenize_with_extra`) → mentions upsertées → alias expand → jauge →
  `process_spans`. **Le core reste la source de vérité de la tokenisation.**
- `Span` : derive `PartialEq` ajouté (tests).

### `cloison-proxy`
- `config.rs` : `SessionConfig` + env (`CLOISON_ALIAS_EXPANSION`=1,
  `CLOISON_QUASI_ID_GAUGE`=0, `CLOISON_QUASI_ID_THRESHOLD`=0.5,
  `CLOISON_ALIAS_MAX_MENTIONS`=200), Debug sans secret.
- `engine.rs` : `RequestEngine::tokenize_session` ; `tokenize_chat_request` /
  `tokenize_completion_request` acceptent `session`/`options` (None = chemin
  historique bit-identique) et renvoient `SessionFlags.quasi_id_flagged`.
- `handlers.rs` : `AppState.session` (mention store par daemon) +
  `session_options` (Some en mode N0 uniquement) ; verrou par requête ;
  drapeau jauge → `Metrics.quasi_id_flags` + `tracing::warn` (jamais de
  texte).
- Tests e2e N0 : `n0_alias_across_requests_masks_derived_form` (msg 1
  « Mamadou » → msg 2 « Momo » masqué par alias, roundtrip) ;
  `n0_quasi_id_gauge_flags_dense_text` (opt-in : flag sur densité
  âge+acte+date+lieu, 0 sans opt-in) ;
  `n0_keychain_service_roundtrip` (chantier ② : service keychain posé →
  boot + roundtrip).

## Chantier ② — keychain OS (passphrase du coffre)

Livré dans cette session (N0V11-PREP §2.2 / §4.4 : « au moins une plateforme
prouvée, fallback env, la passphrase jamais persistée en clair »).

### `cloison-proxy` — `src/passphrase.rs` (nouveau)
- `PassphraseProvider` : résolution au boot **keychain OS → env → fail-loud** :
  - `CLOISON_VAULT_KEYCHAIN_SERVICE` posé → lecture dans le keychain
    (crate `keyring` v3 : Windows Credential Manager / macOS Keychain /
    Linux Secret Service-keyutils — **stockage chiffré par l'OS**) ;
  - **premier démarrage** (entrée absente + env fournie) : l'env est
    **stockée** dans le keychain (création uniquement, jamais d'écrasement —
    le keycheck du coffre reste le fail-loud final) ;
  - **keychain indisponible** (pas de Secret Service / keyutils) : repli
    `CLOISON_VAULT_PASSPHRASE` avec `warn` — jamais un blocage silencieux ;
  - ni keychain ni env → **refus de démarrer** (fail-loud, message inchangé
    pour les tests existants).
- `config.rs` : `N0VaultConfig.keychain_service` / `keychain_user` (env
  `CLOISON_VAULT_KEYCHAIN_SERVICE` / `CLOISON_VAULT_KEYCHAIN_USER` défaut
  `default`) ; fail-loud « coffre sans passphrase » assoupli quand le
  keychain est configuré.
- `handlers.rs` : `AppState::new` résout la passphrase via le fournisseur
  (comportement env inchangé sans keychain).
- `Cargo.toml` : `keyring = "3"`.
- Tests : 5 unitaires passphrase (env, fail-loud, NoEntry+env → stockage,
  NoEntry sans env → fail-loud, roundtrip) + 1 e2e N0 (boot + roundtrip avec
  service keychain posé).

## Comment lancer / tester

```bash
cd cloison && source ~/.cargo/env
cargo test --workspace                 # suites vertes (core + proxy + …)
cargo clippy --workspace --all-targets -- -D warnings   # 0 erreur
cargo fmt --all -- --check             # 0 diff
cargo test -p cloison-core alias::     # tests R1–R7 in-core
cargo test -p cloison-core quasi_id::  # jauge in-core
cargo test -p cloison-proxy --test e2e_n0   # e2e N0 (7/7)
# Daemon N0 : CLOISON_VAULT_PATH=… CLOISON_VAULT_PASSPHRASE=… cloison-proxy
#   (docs/N0.md §3 — alias actif par défaut, jauge opt-in)
```

## Résultats

### Gates (VPS, rustdev rust:1.97) — TOUTES VERTES ✅

- `cargo test --workspace` : **toutes les suites ok, 0 échec** — core lib
  **85 tests** (dont les **17 invariants bloquants inchangés**), proxy
  (**e2e_n0 7/7** — 5 STACK-N0 + 2 v1.1), control, ledger, verify, cli,
  wasm, audit.
- `cargo clippy --workspace --all-targets -- -D warnings` : **0 erreur**.
- `cargo fmt --all -- --check` : **0 diff** (rustfmt 1.97).

### Portage vérifié par les tests
- **Alias** : R1/R2/R3 (prénom seul, titre+nom, nom seul), noms communs
  exclus, pronoms jamais dérivés ni matchés, titre ≠ prénom, R5 diminutifs,
  R6 raccourcis, R4 off par défaut, garde-fou max_derived_forms, score
  plafonné ×0.85 + boost borné (seen_count), R7 casse/diacritiques, dédup
  contre les spans core, déterminisme.
- **Jauge** : densité 4 catégories → flag ; texte clairsemé → 0 ; seuil 1.0
  = désactivée de fait ; fenêtrage ; **zéro valeur / zéro résolution** ;
  signaux dans l'ordre stable des catégories ; texte vide → 0.
- **Moteur** : alias masque un diminutif inter-requêtes (roundtrip) ;
  pronoms jamais masqués ; alias désactivé → no-op ; jauge flag présent /
  absent selon opt-in ; `tokenize_with_extra` (serveur) inchangé.
- **e2e N0** : preuve bout en bout inter-requêtes + drapeau jauge via HTTP.

## Invariants de sécurité vérifiés

- **I1 (zéro clair)** : les alias sont tokenisés comme PERSON/LOC (sentinelles
  ⟦…⟧, jamais de clair amont — vérifié e2e) ; le store de mentions ne
  contient que des clés de mentions, jamais de valeurs de reçus.
- **I3 (restaurer uniquement ce qu'on a émis)** : les alias passent par le
  registre de la requête + MAC — **inchangé** (l'alias n'élargit pas la
  restauration ; 17 invariants core intacts).
- **Jamais les pronoms** : R1–R7 ne dérivent ni ne matchent les pronoms /
  mots-outils (invariant nouveau, testé).
- **Scores plafonnés** : alias ≤ `score_cap × canonique`, boost borné,
  plafond absolu `max_score` (jamais un score gonflé).
- **Jauge signal-only** : compteur + log (aucune valeur, aucune identité
  reconstituée, aucun chaînage — charte §11).
- **Zéro secret / zéro PII** : aucune donnée réelle (jeux synthétiques) ;
  les nouvelles options ne touchent ni aux clés ni aux reçus.

## Questions ouvertes / dette

- **Alias en serveur (paliers N1/N3)** : non câblé — le sidecar Python porte
  déjà l'alias pour les paliers serveur ; le mention store du proxy n'est
  utilisé qu'en mode N0 (décision §4, à re-évaluer avec l'usage réel).
- **NER léger embarqué (④)** : toujours à arbitrer (GO/NO-GO — re-validation
  grille v1.1 obligatoire si le benchmark est touché).
- **`cloison-wasm` (③)** : piste v1.1 suivante (non livrée).
- **Open-core proxy** : le chantier ② ajoute `keyring` au proxy → la
  re-publication open-core du proxy (v0.2.5, deps + tag) est **à faire**
  quand le proxy sera re-publié.
- Limite honnête N0 conservée : un nom **hors gazetteer** et jamais
  mentionné peut partir en clair (docs/N0.md §4.1).

## Porte de sortie (N0V11-PREP §7)

- [x] **Alias intra-session (R1–R7) + jauge quasi-id in-core testés**
      (invariants nouveaux : pronoms jamais masqués, alias bornés, jauge
      signal-only) — portage fidèle du sidecar, zéro réécriture de logique.
- [x] **Serveur bit-identique** : `SessionOptions` séparée de la `Policy` ;
      hors mode N0, aucun changement de comportement ni de `policy_hash`.
- [x] **Tests + portes** : cargo test/clippy/fmt verts, e2e_n0 8/8, 17
      invariants inchangés.
- [x] **Keychain OS opérationnel (chantier ②)** : crate `keyring` v3 (trois
      plates-formes), fallback env avec warn, fail-loud si ni keychain ni
      env ; passphrase jamais persistée en clair par CLOISON.
- [x] **Docs** : `docs/N0.md` (§3, §4, §7), `docs/CONFIG.md`, `README.md`,
      `deploy/install-n0.sh`.
- [x] **Journal + push** (commits `f9a3bb2a`, `76437e65`, + session ②).
- [x] **Re-publication open-core v0.2.4** — core/audit/proxy publiés et
      **vérifiés** (cargo test des tags publiés, rust 1.97, deps git taguées
      v0.2.4 ; le tag core embarque `alias.rs` + `quasi_id.rs`). Procédure
      `docs/OPEN-CORE.md` §4 exécutée via `oc-republish-v4*.sh` (VPS).

## Prochaine étape

**Chantier ③ — `cloison-wasm` navigateur** (tokenize/restore in-browser,
coffre in-memory, page de démo — N0V11-PREP §2.3), puis arbitrage ④ NER
léger embarqué (ONNX, GO/NO-GO en ouverture — re-validation grille v1.1 si
le benchmark est touché), puis re-publication open-core du proxy (v0.2.5,
le chantier ② ajoute `keyring`).
