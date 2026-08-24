# CLOISON — STACK-N0 : Kit moteur léger Rust seul (daemon desktop)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.
> Session N0, 24-25 août 2026. Suite directe de DEPLOY-10 + `journal/N0-PREP.md`.
> La porte de sortie N0-PREP §7 est la cible ; aucune sortie des références
> (charte `Doc_REF/CLOISON-NOTE-TECHNIQUE.md`, handoffs `REPRISE*.md`, `N0-PREP.md`).

## Objectif

Livrer **N0 v1** : le moteur descend chez le client (charte §4) — un **daemon
desktop** compatible OpenAI (`localhost:8787`) qui pseudonymise avant le LLM
et restaure au retour, **sans sidecar NER Python** (moteur Rust seul), avec un
**coffre persistant** chiffré (clé locale, fail-loud) et une **auth 100 %
locale** (zéro dépendance au plan de contrôle pour masquer). Prérequis
N0-PREP §6 : **tous soldés** ✅ (71/75, matricule officiel, 72/79, ONNX prod).

En ouverture de session, le pilote a en outre soldé les 2 décisions pilote en
attente et demandé la vérification des thèmes sombre/clair des pages livrées
(voir §0).

## §0. Pré-session (décisions pilote + thèmes)

### Décisions pilote — SOLDÉES (25/08/2026)

1. **`dsh.wonkom.ai`** : retrait du record A **validé** (recommandation
   DEPLOY-9 actée). Constat 25/08/2026 : le record résout **encore**
   (144.217.81.251, vérifié via résolveurs publics 8.8.8.8) → **action
   opérateur** (zone anycast.me) toujours en attente d'exécution (instruction
   DEPLOY-10 §« Décision pilote requise ») ; rien ne sert ce sous-domaine.
2. **Mode audit** : **interne par défaut validé** — `CLOISON_AUDIT_MODE=0` en
   prod (masquage actif), rapport de conformité k-anonyme par tenant en
   observe-only (opt-in), voie de transparence publique = `journal.wonkom.ai`.
   Pas de rapport public supplémentaire.

Journaux mis à jour : `REPRISE.md` §5, `REPRISE-DEPLOIEMENT.md` §6/§6bis,
`DEPLOY-9.md`, `DEPLOY-10.md` (commit `d9b983bd`).

### Thèmes sombre/clair des pages livrées — VÉRIFIÉS + 2 défauts corrigés

Vérification par harnais Node (DOM simulé, 4 scénarios OS×état × 2 pages) +
cohérence CSS (couleurs en dur, jeux de variables, remaps SVG) + vérification
du déploiement en ligne (depuis le VPS, le HTTPS local étant indisponible) :

- **Page journal (`deploy/journal-html/index.html`, `journal.wonkom.ai`)** :
  la bascule ignorait la préférence OS (1er clic no-op en OS sombre) →
  **alignée sur le design system** (fonction `currentTheme()` consciente de
  l'OS, `window.matchMedia`). Page **redéployée** (hash servi == repo,
  `e4ac5c2c…`), WASM 200, ledger 3 lignes intact.
- **Topologie (`Doc_REF/cloison-topologie_PII_V3.html`)** : 2 couleurs SVG en
  dur **sans remap** (`#AD3B3B` danger, `#CFD3DC` line) ne s'adaptaient pas au
  thème sombre → **remaps ajoutés** (`var(--danger)` / `var(--line)`) +
  `matchMedia` → `window.matchMedia` (robustesse).
- Résultat : **tous les tests de thème passent** (bascule + aria-pressed +
  parité des jeux de variables clair/sombre + zéro couleur SVG orpheline).

## Périmètre

**Dans :** coffre persistant N0 (chemin fichier + clé dérivée d'une passphrase
locale + fail-loud), sel de session persistant, config N0 par défaut (auth
locale, audit opt-in OFF, généralisation active, embeddings bloqué), politique
N0 (généralisation des faibles cardinalités), tests e2e N0, preuve daemon sur
le VPS, `docs/N0.md` + guide client + CONFIG + README, script
`deploy/install-n0.sh`, journal + push, re-publication open-core (procédure
`docs/OPEN-CORE.md` §4).

**Hors :** sidecar NER (paliers serveur — jamais en N0), alias intra-session
R1-R7 et jauge quasi-id **in-core** (report v1.1 documenté), keychain OS
(piste v1.1), déclinaisons wasm/mobile (piste v1.1), embeddings (bloqué).

## Décisions (N0-PREP §4 tranchées en ouverture)

1. **Surface v1 = daemon desktop** (reco pilote §6bis) : endpoint
   OpenAI-compatible `localhost:8787`, binaire unique `cloison-proxy`
   (rôle `edge`), install script `deploy/install-n0.sh`.
2. **Coffre persistant** : `Vault` redb (déjà chiffré AES-256-GCM) rendu
   **persistant et branché** — `CLOISON_VAULT_PATH` l'active ; clé **dérivée
   d'une passphrase locale** (HKDF-SHA256, sel de domaine fixe, jamais
   persistée) ; **fail-loud** : passphrase absente/mauvaise ou coffre corrompu
   → refus de démarrer (entrée de contrôle `__cloison_keycheck__` chiffrée,
   vérifiée au boot — jamais de recréation silencieuse) ; TTL configurable
   (défaut 7 j).
3. **Auth 100 % locale** : `CLOISON_EXPECTED_ACCESS_TOKEN` (temps constant),
   aucun appel au plan de contrôle pour masquer ; audit k-anonyme **opt-in**
   (`CLOISON_AUDIT_MODE=1` + wiring contrôle documenté).
4. **Alias intra-session + jauge quasi-id in-core : report v1.1 documenté**
   (N0-PREP §4.4 — le sidecar les porte déjà pour les paliers serveur ; le
   core v1 reste « le plus léger possible » ; limite honnête §11 charte).
5. **`/v1/embeddings` : bloqué par défaut** (404 — aucune route, charte §7.1 ;
   piste « tokeniser avant embedding » documentée, non livrée).
6. **Généralisation active** : politique `Policy::n0_for` — date → `YYYY-MM`,
   IP → `[IP]`, CB → masque, **ville (gazetteer) → `[VILLE_SN]`** (faible
   cardinalité, jamais de jeton) ; les règles par défaut du `Generalizer`
   couvraient déjà Date/Ip/CB, la ville est explicitement ajoutée pour N0.
7. **Sel de session persistant** (mode N0) : fichier `<vault_path>.salt`
   (0600) — la session du daemon survit aux redémarrages (restauration des
   conversations en cours) ; rotation = suppression du fichier + redémarrage
   (les anciens jetons ne sont plus restaurés, marqueur neutre). Déviation
   apparente à l'invariant I7 (rotation par boot) **justifiée et consignée** :
   en N0 la « session » est la session d'utilisation du daemon desktop
   (longue, maîtrisée par l'utilisateur) ; hors mode N0, le comportement
   historique (sel aléatoire par boot) est **inchangé**.
8. **La restauration reste bornée au registre de la requête** (I3 inchangé) :
   le coffre est la source de valeurs persistante (fallback), jamais une porte
   d'élargissement de la restauration.

## Ce qui a été construit

### `cloison-core`
- `vault.rs` : `derive_key_from_passphrase()` (HKDF, sel de domaine fixe) ;
  **keycheck au boot** (`check_or_seed_key`, entrée `__cloison_keycheck__`
  chiffrée — mauvaise clé/corruption → `CloisonError::Vault` fail-loud) ;
  `impl Clone` (partage Arc redb entre moteurs) ; `random_hex()`.
- `policy.rs` : `Policy::n0_for(tenant_id)` — généralisation des faibles
  cardinalités explicite (Date/Ip/CreditCard/ville_sn).
- `lib.rs` : re-export `derive_key_from_passphrase`.
- Tests : **+5** (roundtrip après réouverture, mauvaise clé fail-loud,
  dérivation déterministe, keycheck semé, politique n0).

### `cloison-proxy`
- `config.rs` : `N0VaultConfig` (`CLOISON_VAULT_PATH`, `CLOISON_VAULT_PASSPHRASE`,
  `CLOISON_VAULT_TTL_SECS`, `CLOISON_SESSION_SALT_FILE`) ; `load_session_salt`
  (1. hex explicite → 2. fichier persistant N0 → 3. aléatoire par boot) ;
  fail-loud si coffre posé sans passphrase ; Debug masque la passphrase.
  Tests : **+3** (création/relecture du sel 0600, mauvaise taille fail-loud,
  hex explicite prioritaire).
- `engine.rs` : `RequestEngine::new(keys, id, vault: Option<Arc<Vault>>)` —
  coffre partagé branché (`Engine::with_vault`).
- `handlers.rs` : `AppState.vault` — ouverture au boot (clé dérivée,
  keycheck), politique N0 si mode N0, passage du coffre aux 2 routes
  (chat + legacy) ; log « mode N0 actif ».
- `tests/e2e_n0.rs` : **5 tests** (roundtrip avec coffre persistant + aucun
  clair dans le fichier + réouverture ; mauvaise passphrase fail-loud ;
  passphrase absente fail-loud ; sel persistant → jetons identiques entre
  redémarrages ; embeddings → 404).
- `tests/e2e.rs` / `e2e_audit.rs` / `e2e_control.rs` : champ `vault` ajouté
  aux configs de test (`N0VaultConfig::default()`).

### Docs / packaging
- `docs/N0.md` : guide daemon desktop (install, config, **limites honnêtes**
  §4 : rappel PERSON/LOC réduit, poste compromis, quasi-identifiants, PII
  hallucinée, embeddings bloqué, restauration bornée), vérification, audit
  opt-in, pistes v1.1.
- `docs/CONFIG.md` : variables N0 (4 nouvelles + `CLOISON_SESSION_SALT_HEX`
  actualisée).
- `docs/CLIENT-GUIDE.md` : §4bis « N0 — daemon local ».
- `README.md` : doc N0 + état d'avancement N0.
- `deploy/install-n0.sh` : install du daemon (build release, `~/.cloison`,
  clé locataire affichée une fois, config minimale).

## Comment lancer / tester

```bash
# Tests + portes (VPS, conteneur rust:1.97 — rustdev) :
cargo test --workspace                 # 30 suites vertes
cargo clippy --workspace --all-targets -- -D warnings   # 0 erreur
cargo fmt --all -- --check             # 0 diff (rustfmt 1.97)
cargo check -p cloison-core -p cloison-verify --features wasm -p cloison-wasm \
  --target wasm32-unknown-unknown      # WASM vert (cible ajoutée dans rustdev)

# E2E N0 :
cargo test -p cloison-proxy --test e2e_n0   # 5/5

# Preuve daemon réel (VPS) :
bash deploy/install-n0.sh             # build + config ~/.cloison
# puis lancer avec CLOISON_VAULT_PATH/CLOISON_VAULT_PASSPHRASE/… (docs/N0.md §3)
```

## Résultats

### Gates (VPS, rustdev rust:1.97)
- `cargo test --workspace` : **30 suites ok, 0 échec** — core **55 unités +
  17 invariants** (les 17 invariants bloquants **inchangés**), proxy
  (e2e + e2e_audit + e2e_control + **e2e_n0 5/5**), control, ledger, verify,
  cli, wasm, audit.
- `clippy -D warnings` : **0 erreur** ; `fmt --check` : **0 diff** (rustfmt
  1.97, formattage appliqué au dépôt).
- **WASM** : `cloison-core` + `cloison-verify --features wasm` +
  `cloison-wasm` compilent `wasm32-unknown-unknown` (cible ajoutée dans
  rustdev).

### Preuve daemon N0 réel (VPS 144.217.81.251, binaire debug, port 18787 —
le 8787 de prod étant occupé par le conteneur edge)
- **Boot #1** : `sel de session N0 généré et persisté (0600)` +
  `mode N0 actif : coffre persistant local (clé dérivée de la passphrase,
  jamais persistée) ttl_s=604800` + écoute `127.0.0.1:18787`.
- **Roundtrip** : requête `Contact: Aminata Diop, user@example.com, tel
  +221 77 123 45 67` → réponse **restaurée** (les 3 valeurs, zéro sentinelle
  résiduelle) ; le mock LLM n'a reçu que des sentinelles.
- **Coffre chiffré** : `grep Aminata vault.redb` → **rien** (aucun clair) ;
  `vault.redb.salt` en **0600** (16 octets).
- **Persistance** : kill + redémarrage (même config) → **roundtrip OK**
  (coffre réouvert avec la même passphrase, sel stable → mêmes jetons).
- **Fail-loud** : mauvaise passphrase → le daemon **refuse de démarrer**
  (`failed to open N0 vault (fail-loud)`), port non servi (HTTP 000).

## Invariants de sécurité vérifiés

- **I1 (zéro clair)** : aucun clair dans le fichier coffre (vérifié par
  grep binaire) ; la passphrase n'apparaît jamais dans les logs (Debug
  masqué) ; aucun secret persisté par le script d'install.
- **I2 (coffre au bord)** : le coffre ne vit que sur la machine du client ;
  zéro dépendance au plan de contrôle pour masquer (auth locale).
- **I3 (restaurer uniquement ce qu'on a émis)** : restauration bornée au
  registre de la requête + MAC — **inchangé** (le coffre est fallback de
  valeurs, pas une porte d'élargissement) ; les 17 invariants core sont verts.
- **I7 (rotation)** : sel aléatoire par boot **hors N0** (inchangé) ; en N0,
  sel persistant avec rotation explicite documentée (déviation justifiée §7).
- **I8 (fail-loud)** : passphrase absente/mauvaise ou coffre corrompu →
  refus de démarrer ; sel de mauvaise taille → refus (jamais de régénération
  silencieuse).
- **I9 (preuve sans texte)** : l'audit N0 reste **opt-in** ; les reçus ne
  contiennent que des compteurs (code inchangé).
- **Zéro PII réelle / zéro secret** : jeux et requêtes de test 100 %
  synthétiques ; la clé locataire de la preuve a été générée en mémoire.

## Questions ouvertes / dette

- **Alias intra-session (R1-R7) + jauge quasi-id in-core** : report v1.1
  documenté (`docs/N0.md` §7) — le sidecar les porte pour les paliers
  serveur.
- **Keychain OS** (Windows Credential Manager / macOS / libsecret) pour la
  passphrase : piste v1.1 (env var en v1).
- **NER léger embarqué** (voie ONNX de `cloison-detect`) : rampe v1.1 pour le
  rappel PERSON/LOC en texte libre (limite honnête N0 v1, `docs/N0.md` §4.1).
- **`/v1/embeddings`** : bloqué (404) ; piste « tokeniser avant embedding »
  documentée, non livrée.
- **`CLOISON_VAULT_TTL_SECS`** : purge par TTL uniquement (aucun GC actif au
  boot) — acceptable v1 (7 j), à re-évaluer avec l'usage réel.
- **DNS `dsh.wonkom.ai`** : décision soldée (retrait validé) — action
  opérateur anycast.me **toujours en attente** (vérifié 25/08/2026).
- GPU (dette ②) : inchangé (baseline ONNX de DEPLOY-8 comme référence).

## Porte de sortie (N0-PREP §7)

- [x] **Binaire daemon N0 utilisable par un humain** : `deploy/install-n0.sh`
      (build + config `~/.cloison`), endpoint `localhost:8787` compatible
      OpenAI, clé composite locale `mn_<jeton>.<clé_amont>`.
- [x] **Preuve e2e locale** (masquage + restauration, hors-ligne) : roundtrip,
      coffre chiffré sans clair, persistance après redémarrage, fail-loud.
- [x] **Limites honnêtes documentées** (`docs/N0.md` §4 — charte §11).
- [x] **Coffre persistant** : chemin configurable + clé dérivée passphrase
      (fail-loud) + tests (roundtrip après redémarrage, invariants 17 verts).
- [x] **Config N0 par défaut** : auth locale, audit opt-in OFF, généralisation
      active, embeddings bloqué.
- [x] **Tests + portes** : cargo test/clippy/fmt verts, e2e_n0 5/5, WASM vert,
      invariants inchangés (17).
- [x] **Journal STACK-N0.md + push GitHub** (voir commits ci-dessous).
- [x] **Re-publication open-core** (core + proxy v0.2.3, procédure
      `docs/OPEN-CORE.md` §4 — graphe des git deps vérifié, leçon DEPLOY-10).

## Prochaine étape

- **v1.1 N0** (documenté) : alias/jauge in-core, keychain OS, NER léger
  embarqué, déclinaisons `cloison-wasm` (navigateur) et mobile embarqué.
- **Décisions pilote restantes** : DNS `dsh.wonkom.ai` (action opérateur
  anycast.me — instruction DEPLOY-10) ; GPU (en attente, baseline ONNX).
- Re-validation GO à chaque évolution du core (règle §5 de la grille) —
  le mode N0 ne touche pas la stack serveur (benchmark inchangé).
