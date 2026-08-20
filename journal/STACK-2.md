# CLOISON — STACK-2 : cloison-core (Rust)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Implémenter le **cœur déterministe portable** (natif + WASM) du proxy : détection structurée,
tokenisation à clé avec registre d'émission, généralisation des faibles cardinalités, coffre
chiffré — avec les invariants de sécurité de la charte **vérifiés par tests bloquants** et un
**différentiel contre la baseline Presidio** du STACK-1.

## Périmètre

**Dans :** `cloison-core` (Rust, `crates/cloison-core/`) — détection (email, TEL +221, CNI
sénégalaise Luhn, carte bancaire Luhn, IP, date, gazetteers Aho-Corasick), jetons
HMAC+sel+blake3, registre d'émission par requête, coffre redb+AES-256-GCM, généralisation,
moteur tokenize/restore, bindings WASM, tests d'invariants, différentiel Presidio.

**Hors :** le proxy HTTP (STACK-3), le NER transformer PERSON/LOC (STACK-6), le plan de
contrôle (STACK-5).

## Décisions

1. **Sentinele** : `⟦body_b32·tag_b32⟧` (U+27E6/U+27E7, séparateur U+00B7), body = HMAC-SHA256
   (clé_locataire ‖ sel_session, canonical(valeur)) tronqué 8 octets, tag = BLAKE3_keyed
   (clé_mac_session, TYPE ‖ body) tronqué 8 octets, encodage base32 RFC4648 minuscule.
2. **Coffre** : redb (KV pur-Rust) + chiffrement applicatif AES-256-GCM, clé dérivée HKDF,
   nonce aléatoire 12 octets, format `nonce ‖ ciphertext ‖ tag`. Purge par session (TTL).
3. **Registre d'émission** : par requête, en mémoire ; `restore` n'accepte qu'un jeton dont
   le body ∈ registre ET dont le tag est valide — fail-loud sinon (marqueur + compteur).
4. **Faibles cardinalités** : généralisation (âge par tranches de 5 ans, date mois+année) ou
   suppression ; jamais de jeton (fréquence trahit).
5. **Formatage** : `#[warn(missing_docs)]` + clippy `-D warnings` zéro warning (CI bloquante).
6. **WASM** : getrandom feature `js` pour wasm32 ; les fonctions wasm-bindgen exportées sont
   gated derrière `cfg(target_arch = "wasm32")`.

## Ce qui a été construit

- `src/detection.rs` — `Detector` : regex compilées (email, TEL +221, CNI 13 chiffres Luhn,
  CB Luhn, IPv4, date), Aho-Corasick gazetteers, `validate_luhn` publique, `Span`.
- `src/token.rs` — `SessionKeys` (HKDF), `TokenBody`, `Sentinel` (format+parse), `canonicalize`
  (NFC + lowercase + trim), `compute_mac` (BLAKE3 keyed).
- `src/registry.rs` — `IssuanceRegistry` : HashSet body + mapping inverse, snapshot sérialisable.
- `src/vault.rs` — `Vault` : redb + AES-256-GCM (natif), HashMap chiffré (WASM), TTL, purge.
- `src/generalize.rs` — `Generalizer` : Mask/Range/Replace/AgeBucket/DateBucket/Suppress.
- `src/policy.rs` — `Policy`, `DetectorPolicy`, `SubstitutionMode`.
- `src/engine.rs` — `Engine::tokenize` / `Engine::restore`, `RestoreCounters`.
- `src/wasm.rs` — bindings wasm-bindgen (session, tokenize, restore, detect, derive_keys).
- `src/bin/detect_cli.rs` — CLI stdin→spans JSON (pour le différentiel).
- `tests/invariants.rs` — 17 tests d'invariants bloquants.
- `bench/cloison-bench/differential.py` — différentiel core vs Presidio (jeu STACK-1).

## Comment lancer / tester

```bash
cd cloison
source ~/.cargo/env
cargo test -p cloison-core                 # 38 tests unit
cargo test -p cloison-core --test invariants  # 17 invariants bloquants
cargo clippy -p cloison-core -- -D warnings   # zéro warning
cargo check -p cloison-core --target wasm32-unknown-unknown  # WASM
# Différentiel Presidio :
cd bench/cloison-bench && source .venv/bin/activate
python3 differential.py
```

## Résultats

- **Compilation** : natif OK, WASM (`wasm32-unknown-unknown`) OK.
- **Tests** : 55/55 verts (38 unit + 17 invariants).
- **Invariants vérifiés** : roundtrip `restore(tokenize(x)) == x` ; aucun clair sortant ;
  anti-collision (sentinele forgée jamais restaurée) ; déterminisme intra-session ; rotation
  inter-sessions (sel) ; Luhn valide/invalide.
- **Clippy** : 0 erreur, 0 warning (après corrections : parité Luhn, API base32/redb, docs).
- **Différentiel Presidio** (200 docs du jeu STACK-1) : le core détecte du structuré que
  Presidio rate (MAIL 153, TEL 63, CNI 28 en « core seul ») ; Presidio détecte PERSON/LOC
  (354/327) que le core ne couvre pas encore (NER = STACK-6) et quelques MAIL/TEL/CNI avec
  bornes différentes (exact-match strict → divergence). **Outil de diagnostic, pas un score.**

## Invariants de sécurité vérifiés

1. **Zéro PII sur le plan de contrôle** : le coffre ne vit qu'à l'edge (natif ou WASM) ; le
   cloud n'a aucun accès (STACK-5 construira le plan de contrôle aveugle).
2. **Restaurer uniquement ce qu'on a émis** : `restore` exige `IssuanceRegistry.contains(body)`
   ET `verify_mac` — testé par l'invariant anti-collision.
3. **Fail-loud** : un échec de restauration incrémente `incomplete`/`blocked`, jamais de jeton
   brut ni de mauvaise valeur émise (testé).
4. **Déterminisme/rotation** : HMAC(clé_locataire ‖ sel_session, valeur) — testé (même
   valeur → même jeton ; sel différent → jeton différent).
5. **Généralisation** : les règles AgeBucket/DateBucket/Suppress ne passent jamais par le jeton.

## Questions ouvertes / dette

- Le core détecte le **structuré** uniquement ; PERSON/LOC par NER est STACK-6 (cloison-detect).
- La longueur des jetons (8+8 octets) est un choix ; à valider contre la robustesse aux
  collisions sur gros volumes (fuzz en STACK-7).
- `results_differential.json` est versionné comme artefact de diagnostic ; régénéré à chaque
  évolution du core.

## Porte de sortie

- [x] cloison-core compile natif + WASM.
- [x] 55 tests verts dont 17 invariants bloquants.
- [x] clippy -D warnings : zéro erreur.
- [x] Différentiel Presidio exécuté et consigné.
- [ ] Build WASM distribué via cloison-wasm : à finaliser en STACK-7 (packaging).

## Prochaine étape

**STACK-3 — `cloison-proxy` (Axum)** : `/v1/chat/completions` non-stream puis stream
(buffer-and-scan), clé composite `mn_<jeton>.<clé_amont>`, forwarding amont, restauration des
jetons émis uniquement, tool-calls inclus. E2E contre LLM mock + réel.
