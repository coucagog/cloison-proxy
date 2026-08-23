# CLOISON — Onboarding locataire (N3)

> Flux bout en bout : créer un locataire, générer sa clé composite, la livrer
> au client, vérifier que l'authentification fonctionne. C'est la couche
> « un client peut nous acheter » (journal `REPRISE-DEPLOIEMENT.md` §6 ②).
>
> Outil principal : `cloison-cli` (crate `crates/cloison-cli`) — enveloppe
> sûre de l'API admin du plan de contrôle. Zéro PII, zéro secret en log :
> le clair `mn_` n'est affiché **qu'une seule fois**, à l'émission.

## 1. Prérequis

- Le plan de contrôle tourne (compose, profil `db`) et l'API admin est
  joignable depuis le poste d'administration (réseau interne ou tunnel SSH —
  THREAT-MODEL §3.1 : `cp`/control ne sont **jamais** publics).
- `cloison-cli` compilé : `cargo build -p cloison-cli` (workspace) — ou
  binaire `target/debug/cloison-cli`.
- URL du contrôle : `CLOISON_CONTROL_URL` (défaut `http://127.0.0.1:8788`).
  Aucun secret dans l'URL.

## 2. Flux d'onboarding (scripté)

Le script `deploy/onboard_client.sh` exécute l'intégralité du flux :

```bash
# Depuis l'hôte du VPS (ou un poste autorisé) :
CLOISON_CONTROL_URL=http://127.0.0.1:8788 \
  ./deploy/onboard_client.sh acme "Acme SARL" pro
```

Ce que fait le script :

| Étape | Action | Sortie |
|---|---|---|
| 1. Provision | `POST /admin/tenants` (tenant + licence `plan`) | Tenant JSON |
| 2. Émission | `POST /admin/tenants/{id}/tokens` | `TokenIssued` : le clair `mn_` (affiché UNE fois) |
| 3. Vérification | `POST /v1/control/verify` avec `hex(SHA-256(domaine ‖ clair))` | `valid: true` |
| 4. Livraison | Affiche la **clé composite** prête pour l'interface IA du client | `Base URL` + `Clé` |

Le script ne journalise **jamais** le clair ; il le hachait en mémoire pour
l'étape 3 et l'oublie.

## 3. Commandes équivalentes à la main (`cloison-cli`)

```bash
# 1. Créer le tenant + licence :
cloison-cli provision acme --nom "Acme SARL" --plan pro

# 2. (Option) émettre un second jeton (ex. rotation de poste) :
cloison-cli token issue acme

# 3. Vérifier qu'un jeton est valide (le clair ne quitte jamais le CLI) :
cloison-cli token verify acme mn_<jeton>

# 4. Rotation (l'ancien jeton reste valide `grace_period_secs`, défaut 300 s) :
cloison-cli token rotate acme tok-<id>

# 5. Révocation immédiate (perte/compromission) :
cloison-cli token revoke acme tok-<id>

# 6. Publier une politique par locataire (JSON depuis fichier ou stdin) :
cloison-cli policy set acme /path/policy.json

# 7. Ajouter une licence avec expiration :
cloison-cli license add acme --plan enterprise --expires_at 1893456000

# 8. Voir la santé du journal public :
cloison-cli ledger root
```

## 4. Livraison au client — la clé composite

La clé composite est la seule chose que le client configure (guide client :
`docs/CLIENT-GUIDE.md`) :

```
Base URL : https://api.wonkom.ai/v1
Clé      : mn_<jeton_acces>.<cle_amont_du_client>
```

- `mn_<jeton_acces>` : identifie/autorise le locataire auprès du plan de
  contrôle (vérifié par hash — le clair ne transite jamais).
- `<cle_amont_du_client>` : la clé du fournisseur LLM du client
  (ex. `sk-proj-…`), transmise au fournisseur **uniquement en header**.

## 5. Vérifications post-onboarding

- **Auth** : `curl -s -o /dev/null -w '%{http_code}' https://api.wonkom.ai/v1/models`
  avec la clé composite → `200` ; sans clé → `401` ; jeton inconnu → `401`.
- **Requête réelle** : un `POST /v1/chat/completions` avec la clé composite
  répond normalement ; la PII envoyée dans le prompt ne part **jamais** en
  clair vers le fournisseur (e2e anti-pass-through, `deploy/e2e_reel.sh`).
- **Journal public** : `https://journal.wonkom.ai/ledger.jsonl` s'enrichit
  des fenêtres d'audit (compteurs k-anonymes contresignés) — la preuve
  « nous ne lisons pas » reste vérifiable par le client (WASM `cloison-verify`).
- **Rapport de conformité** (mode audit observe-only activé chez le client) :
  `GET /v1/audit/report?period=weekly` — rapport k-anonyme signé, présentable
  (voir `docs/CLIENT-GUIDE.md` §6).

## 6. Sécurité (rappel des invariants)

- **I2** : le stockage du contrôle ne contient que des hash ; le clair n'est
  affiché qu'à l'émission. `cloison-cli verify` ne transmet que le digest.
- **I9 / O2** : aucun texte client, aucun compteur < k dans le journal.
- **Zéro secret en log** : le CLI n'imprime jamais le clair sauf à
  l'émission (une fois, avec avertissement explicite).
- **IDOR** : les routes admin vérifient l'appartenance tenant/token
  (`cloison-control`, tests dédiés) — le CLI ne peut pas y déroger.
