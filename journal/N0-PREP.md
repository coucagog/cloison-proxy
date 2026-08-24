# CLOISON — N0-PREP : Préparation de la session N0 (kit moteur léger Rust seul)

> Handoff de préparation, écrit en fin de DEPLOY-10 (24/08/2026). À lire
> AVANT la session N0. Complète `REPRISE.md` §6 (PRIORITÉ 6), 
> `REPRISE-DEPLOIEMENT.md` §6bis (décisions pilote N0) et la charte
> `Doc_REF/CLOISON-NOTE-TECHNIQUE.md` (§4 échelle N0-N3, §5.1, §7.2).

---

## 1. Pourquoi N0 (rappel de la charte)

N0 = **le moteur descend chez le client** : l'opérateur ne lit rien, la
promesse absolue. Le cloud ne reste qu'un plan de contrôle aveugle. C'est
la **cible v1** de la charte (§4) — l'architecture edge qui justifie le
produit face aux concurrents hébergés (N3 = entrée de gamme, jamais
l'argument).

**Honnêteté N0 (non négociable)** : ne protège pas contre un poste
compromis (coffre + clé vivent sur la machine) — à écrire dans la promesse
et le THREAT-MODEL (déjà documenté).

## 2. Périmètre N0 v1 (décisions pilote §6bis — à confirmer en ouverture)

**Recommandation pilote : déclinaison DAEMON DESKTOP en v1** — un endpoint
OpenAI-compatible `localhost:8787` qui réutilise `cloison-proxy` + un coffre
persistant local. Les autres déclinaisons (mobile embarqué, navigateur
WASM) suivent, même moteur.

**Le plus léger possible** : moteur **Rust seul** (`cloison-core` : détection
structurée + gazetteers + Luhn), **SANS sidecar NER Python** (charte §4 : le
sidecar est pour les paliers serveur/enclave). Conséquence honnête à
documenter : **rappel PERSON/LOC en texte libre réduit** (pas de NER
transformer local) — le fossé GO (PERSON 0.94 / LOC 0.84) est porté par le
sidecar ; N0 v1 s'appuie sur les gazetteers + regex (le rappel PERSON/LOC
sur noms de gazetteer reste bon ; le texte libre est la limite assumée).

## 3. État de l'existant à réutiliser (ZÉRO réécriture)

| Composant | État | Usage N0 |
|---|---|---|
| `cloison-core` (lib Rust, natif + WASM) | ✅ 50 unit + 17 invariants | LE moteur : `Engine::tokenize/restore`, `Detector` (regex + gazetteers + Luhn), `Vault` (redb), `SessionKeys` |
| `cloison-proxy` (Axum) | ✅ e2e mock 12/12 + réel | Réutilisé TEL QUEL pour l'endpoint OpenAI (CLOISON_ROLE=edge déjà vérifié) |
| `cloison-wasm` | squelette | Déclinaison navigateur (plus tard) |
| Coffre | `Vault` redb + AES-256-GCM (natif) | **à rendre persistant** (chemin fichier configurable) + clé locale keychain |

## 4. Décisions techniques à trancher en ouverture (design §6bis)

1. **Surface v1** : daemon desktop (reco) — endpoint `localhost:8787`
   OpenAI-compatible. Packaging : binaire unique + install script (ex.
   `cargo install` ou release GitHub + script) ? Windows/macOS/Linux ?
2. **Coffre persistant** : emplacement (répertoire utilisateur standard,
   ex. `~/.cloison/`), clé dérivée d'une **passphrase/keychain locale**
   (pas de clé en clair sur disque), perte de clé = **fail-loud** (jamais
   de restauration silencieuse).
3. **Auth 100 % locale** : jeton `mn_` résolu localement (auth statique
   existante du proxy en mode N0) — **zéro dépendance au plan de contrôle
   pour masquer**. L'audit k-anonyme vers le journal reste **opt-in**.
4. **Alias intra-session (R1-R7) + jauge quasi-id dans le core v1** (léger,
   déterministe) **ou report v1.1 documenté** — à trancher (le sidecar les a
   déjà ; le core les porterait pour N0).
5. **`/v1/embeddings`** : charte §7.1 — cas sensible. Décision N0 : bloquer
   par défaut (policy) ou tokeniser avant embedding (perte de sens
   assumée) — documenté.
6. **Généralisation des faibles cardinalités** : déjà dans le core
   (`Generalizer` : AgeBucket/DateBucket/Suppress) — inclure dans la
   politique par défaut N0.

## 5. Déroulé proposé de la session N0 (ordre)

1. **Ouverture** : valider les décisions §4 avec le pilote (surface v1,
   coffre, alias/jauge in-core ou v1.1, embeddings).
2. **Coffre persistant** : `Vault` → chemin fichier + clé locale
   (keychain/passphrase), perte = fail-loud, tests (invariants roundtrip
   après redémarrage).
3. **Daemon** : packaging du binaire (proxy + core en lib), config N0 par
   défaut (auth locale, audit opt-in OFF, généralisation active), test e2e
   local (mock LLM sans réseau).
4. **Limites honnêtes** : doc `docs/N0.md` ou section CLIENT-GUIDE —
   rappel PERSON/LOC réduit sans sidecar, poste compromis, jauge quasi-id
   (signal, pas résolution).
5. **Déclinaisons (si le temps)** : `cloison-wasm` (navigateur) — la
   mécanique WASM existe déjà (cloison-verify), à étendre au core.
6. **Tests + portes** : cargo test/clippy/fmt verts, e2e local, invariants
   inchangés (17), journal `STACK-N0.md` + push.

## 6. Prérequis — TOUS SOLDÉS ✅ (sessions précédentes)

- Dette 71/75 **réglée** (DEPLOY-9) ; **71 confirmé Orange** (DEPLOY-10
  addendum) ; 72/79 couverts (DEPLOY-10).
- Matricule au format officiel (DEPLOY-10) ; open-core v0.2.2 republié et
  vérifié ; CI verte ; stack prod saine (ONNX int8).
- GPU : toujours en attente (sans objet pour N0 — moteur Rust léger).

## 7. Sortie attendue de la session N0

- Binaire daemon N0 utilisable par un humain (install + endpoint
  `localhost:8787` + clé composite locale `mn_<jeton>.<clé_amont>`).
- Preuve e2e locale (masquage + restauration, hors-ligne).
- Limites honnêtes documentées (charte §11).
- Journal `STACK-N0.md` + push + (si code core modifié) re-publication
  open-core.
