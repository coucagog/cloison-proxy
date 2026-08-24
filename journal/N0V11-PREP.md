# CLOISON — N0V11-PREP : Préparation de la session N0 v1.1

> Handoff de préparation, écrit en fin de STACK-N0 (25/08/2026). À lire
> AVANT la session N0 v1.1. Complète `journal/STACK-N0.md` (§Prochaine
> étape, §Questions ouvertes), `docs/N0.md` (§7 pistes v1.1), la charte
> `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§6.1, §11, §16) et les handoffs
> `REPRISE.md` / `REPRISE-DEPLOIEMENT.md`.

---

## 1. Pourquoi N0 v1.1

N0 v1 est **livré** (STACK-N0) : daemon desktop `localhost:8787`, coffre
persistant (passphrase locale, fail-loud), auth 100 % locale, sel persistant,
généralisation active, embeddings bloqué, limites honnêtes documentées
(`docs/N0.md`). **v1.1 = colmater les limites assumées de v1** sans sortir du
cadre « moteur Rust seul, le plus léger possible » (charte §4, §11).

Les trois limites v1.1 ciblées (par ordre de valeur produit) :
1. **Rappel PERSON/LOC en texte libre** : les gazetteers couvrent les noms
   connus, mais un nom hors liste (ou une ville hors liste) part en clair.
   Deux leviers documentés : alias intra-session (masquer les variantes
   d'un nom déjà détecté) et NER léger embarqué (ONNX).
2. **Passphrase en variable d'environnement** : fonctionnelle mais fragile
   (processus, scripts). Le keychain OS est la voie propre.
3. **Pas de déclinaison navigateur** : `cloison-wasm` est un squelette ;
   le core compile déjà WASM (vérifié STACK-N0).

## 2. Périmètre N0 v1.1 (chantiers, ordre recommandé)

> **Recommandation : ordre ① alias+jauge → ② keychain → ③ wasm navigateur →
> ④ NER léger embarqué (le plus coûteux, à arbitrer avec la cible produit).**

1. **① Alias intra-session (R1-R7) + jauge quasi-id IN-CORE** (léger,
   déterministe). Le sidecar `cloison-detect` les porte déjà pour les paliers
   serveur (STACK-6 : règles R1-R7, jauge fenêtrée) ; il s'agit de porter
   l'équivalent **in-core** (Rust seul) pour N0 :
   - Alias : « Marie Dupont » détecté → masquer aussi « Marie », « Mme
     Dupont », formes dérivées **dans la conversation** (jamais les pronoms —
     charte §6.1 couche 4). État d'alias : par session (où ? mémoire du
     daemon vs coffre — à trancher).
   - Jauge quasi-id : densité (âge + acte + date + lieu) fenêtrée,
     `flagged = score > seuil` (charte §6.1 couche 6, §11 : signal, pas
     résolution). Sortie : compteur / flag, jamais une « résolution ».
   - **Réutilisable** : `services/cloison-detect/src/alias.py` +
   `quasi_id.py` (référence de règles) ; `crates/cloison-core/src/
   generalize.rs` (AgeBucket/DateBucket existants).
2. **② Keychain OS** pour la passphrase du coffre : Windows Credential
   Manager / macOS Keychain / Linux libsecret — à la place de
   `CLOISON_VAULT_PASSPHRASE` (qui reste le fallback). Crate candidat :
   `keyring` (multi-plateforme). Périmètre v1.1 : au moins **une plateforme
   prouvée** (reco : Linux libsecret + fallback env, la prod étant Linux).
3. **③ Déclinaison navigateur `cloison-wasm`** : la mécanique WASM existe
   (`cloison-verify`, le core compile `wasm32-unknown-unknown`) ; à étendre
   à un module navigateur `@cloison/core` (tokenize/restore in-browser).
   Coffre navigateur : **in-memory** (aucune valeur persistée en clair) —
   IndexedDB chiffré = décision à trancher (complexité, clé de chiffrement
   navigateur sans keychain = limite).
4. **④ NER léger embarqué (ONNX)** : exporter un détecteur PERSON/LOC léger
   en ONNX int8 et l'embarquer dans le daemon (remplace le vide actuel pour
   le texte libre). **Attention** : c'est le chantier le plus lourd (taille
   du modèle, latence CPU, re-validation GO — règle §5 de la grille si le
   benchmark est touché). À arbitrer : modèle de petite taille (GLiNER-base
   ou équivalent) vs gain de rappel mesuré. **Décision d'arbitrage requise
   en ouverture** — la voie ONNX de `cloison-detect` (DEPLOY-8) est la
   référence technique.

## 3. État de l'existant à réutiliser (ZÉRO réécriture)

| Composant | État (fin STACK-N0) | Usage v1.1 |
|---|---|---|
| `cloison-core` | 72 tests (55 unit + 17 invariants), WASM ✅ | Alias + jauge in-core ; base wasm |
| `Vault` persistant | redb + AES-256-GCM, clé dérivée passphrase, keycheck fail-loud | Stockage éventuel de l'état d'alias (décision ①) |
| `Policy::n0_for` | généralisation active (Date/IP/CB/ville_sn) | Étendre si l'alias ajoute des types |
| `cloison-detect` (Python) | alias.py + quasi_id.py (référence de règles R1-R7, jauge) | **Portage** de la logique (pas de copie Python) |
| `cloison-verify` / core | build WASM vérifié STACK-N0 | Base de `cloison-wasm` navigateur |
| `docs/N0.md` §7 | pistes v1.1 documentées | Source du périmètre |
| Open-core | core/audit/proxy v0.2.3 publiés + vérifiés | Re-publier si le core change (v0.2.4) |

## 4. Décisions techniques à trancher en ouverture

1. **Ordre des chantiers** : reco ①②③④ (l'alias+jauge est le gain produit
   immédiat ; le NER léger est lourd et à arbitrer). Confirmer avec le pilote.
2. **Alias — état et périmètre** : intra-requête (aucun état) vs
   intra-session (état en mémoire du daemon vs coffre) ; règles R1-R7
   portées (prénom seul, Mme X, formes dérivées — quelles bornes) ; score
   plafonné (réf. sidecar : garde ≥ 2 tokens, score ≤ 0.85 du canonique) ;
   **jamais les pronoms**.
3. **Jauge quasi-id — formule** : densité fenêtrée (réf. sidecar), seuil
   (défaut = désactivée de fait à 1.0 ?) ; signal = compteur + flag (jamais
   une résolution, charte §11).
4. **Keychain** : crate `keyring` (multi-plateforme) vs appels natifs ;
   plateforme cible v1.1 ; comportement sans keychain (fallback env, warn) ;
   **la passphrase ne doit JAMAIS être persistée en clair** (invariant).
5. **NER léger (④)** : GO/NO-GO en ouverture — modèle (taille, latence CPU
   cible), embarquement ONNX (artefacts, licence), **re-validation GO
   (grille v1.1, règle §5) obligatoire** si le benchmark est touché.
6. **WASM navigateur (③)** : périmètre minimal (tokenize/restore
   in-browser), coffre in-memory (0 valeur persistée), intégration page
   (exemple HTML), **zéro secret dans la page**.

## 5. Déroulé proposé de la session N0 v1.1 (ordre)

1. **Ouverture** : valider l'ordre des chantiers + les décisions §4
   (alias/jauge in-core ou report, keychain plateforme, NER léger GO/NO-GO).
2. **① Alias + jauge in-core** : portage Rust (léger, déterministe),
   branchement dans `Engine::tokenize` (masquage des formes dérivées dans la
   conversation) + jauge (compteur/flag), tests (invariants : jamais les
   pronoms, alias borné, jauge sans prétention), invariants 17 inchangés.
3. **② Keychain OS** : crate + implémentation (1 plateforme + fallback env),
   tests (roundtrip passphrase, échec fail-loud), docs.
4. **③ `cloison-wasm`** : module navigateur (tokenize/restore), coffre
   in-memory, page de démo, build WASM + vérification.
5. **④ NER léger embarqué** (si GO) : export ONNX, embarquement, re-validation
   GO (grille v1.1), mesure latence — sinon report documenté.
6. **Portes** : cargo test/clippy/fmt verts, WASM, invariants 17, e2e local,
   journal `STACK-N0V11.md` + push + re-publication open-core (v0.2.4 si le
   core change — procédure `docs/OPEN-CORE.md` §4, graphe core→audit→proxy).

## 6. Prérequis — TOUS SOLDÉS ✅ (session STACK-N0)

- CI GitHub verte (run 0a5708c, 10/10 jobs) ; stack prod saine (edge/control/
  detect/journal/postgres, ONNX int8, ledger 3 lignes) ; memwatch 0 OOM.
- Open-core v0.2.3 (core/audit/proxy) publié + vérifié (core 72, audit 34,
  proxy 42 dont e2e_n0 5/5).
- Décisions pilote soldées (25/08/2026) : `dsh.wonkom.ai` retrait DNS validé
  (**action opérateur anycast.me toujours en attente** — vérifié : record
  encore présent) ; mode audit interne par défaut validé.
- Thèmes sombre/clair des pages livrées : vérifiés + corrigés + redéployés
  (STACK-N0 §0, harnais versionné `deploy/theme-check/`).

## 7. Sortie attendue de la session N0 v1.1

- **Alias intra-session + jauge quasi-id in-core** testés (invariants
  nouveaux : pronoms jamais masqués, alias bornés, jauge signal-only) —
  OU report documenté avec justification.
- **Keychain OS** opérationnel (≥ 1 plateforme, fallback env) — OU décision
  documentée.
- **`cloison-wasm`** navigateur minimal prouvé (si retenu) — OU report.
- **NER léger embarqué** : décision GO/NO-GO actée en ouverture ; si GO,
  re-validation GO (grille v1.1) + mesure latence.
- Journal `STACK-N0V11.md` + push + (si core modifié) open-core v0.2.4.

## 8. Dettes transverses (hors N0 v1.1, à surveiller)

- **GPU (dette ②)** : toujours en attente (baseline ONNX de DEPLOY-8 comme
  référence) — décision d'infra pilote.
- **DNS `dsh.wonkom.ai`** : suppression = action opérateur (zone anycast.me,
  instruction DEPLOY-10 §« Décision pilote requise ») — le record est
  toujours présent au 25/08/2026.
- **Formats passeport / permis** : à confirmer auprès de sources normatives
  (détection contextuelle conservée — DEPLOY-10).
- **Calibration fine des seuils en prod** : procédure documentée
  (`measure_clusters.py`), à exécuter avec du trafic réel.
