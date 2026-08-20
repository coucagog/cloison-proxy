# CLOISON — Sécurité

> Périmètre : proxy de confidentialité PII (edge), plan de contrôle aveugle
> (control), sidecar de détection (detect), journal de transparence (ledger),
> distribution WASM (core/verify). Hôte de dev : wonkom.ai.
> Ce document décrit les invariants **implémentés et testés** dans le code —
> chaque invariant est verrouillé par un test (unitaires, invariants, e2e) ou
> par la CI.

## 1. Périmètre & modèle de confiance

| Composant | Confiance | Raison |
|---|---|---|
| `cloison-core` / `cloison-proxy` (edge) | haute | voit la PII en clair (tokenisation) ; exécuté en conteneur durci non-root, read-only |
| `cloison-detect` | haute (transitoire) | voit le texte brut en mémoire (détection) ; **stateless** : ne persiste ni ne tokenise rien |
| `cloison-control` / `cloison-ledger` | moyenne | ne stocke **que des hash et des compteurs** ; le clair `mn_` n'existe que dans la réponse d'émission |
| `cloison-verify` (WASM) | faible | public, stateless, aucun secret requis |
| Fournisseur LLM (OpenRouter/DeepSeek) | non fiable | ne reçoit que du texte tokenisé (sentinelles) + clé amont en header |

Frontières de confiance : le réseau interne compose/K8s (`internal: true`),
le conteneur lui-même (non-root, read-only), et la frontière « texte clair »
du proxy (seul le edge voit le clair).

## 2. Invariants applicatifs (STACK-3, `cloison-proxy` + `cloison-core`)

| # | Invariant | Garantie dans le code |
|---|---|---|
| I1 | **Aucun secret en log** | `CompositeKey::Debug` écrasé (jamais la clé amont) ; clé uniquement en header `Authorization` amont, jamais en URL/query/corps ; logs de tokenisation limités à `body_b32` + `kind_tag` (jamais `plain_value`) ; corps d'erreur amont tronqué (512 o) dans les logs |
| I2 | **Restauration uniquement des jetons émis** | registre d'émission = périmètre de la requête (purge début/fin) ; `restore` exige présence au registre **et** MAC valide ; sentinelle étrangère/forgeuse → bloquée |
| I3 | **Fail-loud** | jeton non résolu → `[REDACTED]` + `metrics.unresolved_tokens` + log warn ; clé malformée → 401 sans appel amont ; erreur amont → 502/504 ; erreur en stream → `data: {"error":…}` puis `[DONE]` |
| I4 | **Tampon borné** | buffer ≤ `max_token_len - 1` octets par canal ; cap dur 256 ; aucune sentinelle partielle vers le client |
| I5 | **JSON des tool_calls toujours valide** | tokenisation/restauration sur la chaîne `arguments` : sentinelles sans guillemets/échappements → JSON valide des deux côtés |
| I6 | **Pass-through conservateur** | champs inconnus (aller/retour) transmis intacts ; seuls `content`/`arguments`/`text`/`prompt` sont transformés |
| I7 | **Rotation de session** | `CLOISON_SESSION_SALT_HEX` absent → sel aléatoire par boot → jetons différents entre redémarrages |
| I8 | **Échec = échec** | aucune transformation silencieuse : toute erreur interne de `cloison-core` remonte en 500 avec `request_id` |

Verrous de base (STACK-2, `crates/cloison-core/tests/invariants.rs`) :
roundtrip `restore(tokenize(x)) == x`, aucune valeur claire dans le texte
tokenisé, sentinelle forgée jamais restaurée, déterminisme (même valeur +
même session → même jeton), rotation (session différente → jeton différent),
Luhn (CNI valide détectée, invalide rejetée).

## 3. Invariants du mode audit (STACK-4, `cloison-audit`)

| # | Invariant |
|---|---|
| I-A1 | Observe-only : corps aller et réponse émis **identiques** à l'entrée (non-stream et stream, testé byte à byte) |
| I-A2 | Reçu sans texte : `Counters` = entiers uniquement ; aucune API n'expose de span/valeur |
| I-A3 | Signature : `verify(pubkey)` vrai sur reçu signé ; faux sur toute altération ; faux sur clé différente |
| I-A4 | Canonicalité : `signing_bytes()` déterministe (JSON compact, clés triées) |
| I-A5 | Corpus séparé : aucune fonction texte→sortie persistante dans le module audit |
| I-A6 | K-anonymat : cellule publiée ⇒ `requests ≥ k ∧ count ≥ k` ; tenant publié ⇒ `request_count ≥ k` ; `session_ref` absent du rapport |
| I-A7 | Fail-closed du mode : un en-tête `X-Cloison-Mode: mask` est ignoré en audit ; `effective_mode` non rétrograble |
| I-A8 | Reçu vérifiable hors-ligne : `cloison-verify` accepte un reçu produit par le proxy |
| I-A9 | Aucune fuite de clé : `Debug` de `AgentKeys`/`Config` ne montre jamais la graine |

## 4. Invariants opérationnels (STACK-7, déploiement)

| # | Invariant | Application |
|---|---|---|
| O1 | **Clés jamais en log, URL ou corps** | `.env`/secrets K8s uniquement ; clé composite en header ; `RUST_LOG` ne contient pas de secret ; e2e réel passe la clé par environnement |
| O2 | **Aucune PII en log** | logs = `request_id`, compteurs, `body_b32`, `kind_tag` ; jamais le texte |
| O3 | **Non-root + read-only** | images distroless (uid 65532), detect uid 10001 ; `read_only: true` + `tmpfs /tmp` partout (compose ET Helm) ; `cap_drop: ALL`, `no-new-privileges` |
| O4 | **Zéro PII réelle dans les tests/bench** | dataset STACK-1 100 % synthétique (seed 42) ; tests detect avec stubs hors-ligne (`CLOISON_OFFLINE=1`) ; e2e réel = PII **simulée** |
| O5 | **SBOM + scan à chaque build** | syft (SPDX) + grype (échec ≥ medium) + trivy (échec ≥ HIGH) sur les 3 images ; signature cosign (OIDC) sur main/tags |
| O6 | **Registre non falsifiable** | ledger append-only : `seq` terminal, `prev_hash` lié, `entry_hash` recomputé, signatures Ed25519 `verify_strict` ; toute altération casse la chaîne |

## 5. Gestion des secrets

- **Développement (wonkom.ai)** : `cp deploy/.env.example .env` — jamais de
  valeur réelle committée. Génération :
  - `CLOISON_TENANT_KEY_HEX=$(openssl rand -hex 32)` (64 hex)
  - `CLOISON_ACCESS_TOKEN=mn_$(openssl rand -hex 16)` (jeton local)
  - `CLOISON_SESSION_SALT_HEX=$(openssl rand -hex 16)` (rotation par boot)
  - `OPENROUTER_API_KEY=sk-or-v1-…` (fournisseur, secret GitHub/CI)
- **Kubernetes** : `deploy/helm/templates/secret.yaml` (ou
  `global.existingSecret` recommandé) ; TLS via ingress/Caddy (voir DEPLOY.md).
- **Rotation** : le sel de session (`CLOISON_SESSION_SALT_HEX`) change les
  jetons à chaque boot ; la rotation des jetons `mn_*` se fait par
  `POST /admin/tenants/{id}/rotate` (l'ancien passe `rotated_at`, plus aucun
  usage). La clé de signature audit (`CLOISON_AUDIT_KEYS`) est générée
  (0600) au boot si absente.
- **CI** : secrets GitHub (`CLOISON_ACCESS_TOKEN`, `OPENROUTER_API_KEY`,
  `CLOISON_TENANT_KEY_HEX`) — jamais dans les logs (l'e2e réel ne les
  affiche pas).

## 6. Chaîne d'outillage (supply chain)

| Outil | Usage | Échec CI |
|---|---|---|
| syft | SBOM SPDX JSON par image (job `images`) | — (artifact) |
| grype | scan vulnérabilités, `severity-cutoff: medium`, `fail-build: true` | oui |
| trivy | scan double, `severity: HIGH,CRITICAL`, `exit-code: 1` | oui |
| cosign | signature OIDC des images poussées (main/tags) | oui |
| `deploy/sbom.sh` | même chaîne exécutable hors CI sur wonkom.ai | — |

Politique : **toute vulnérabilité HIGH/CRITICAL bloque la CI** ; le SBOM est
publié en artifact et joint à l'image.

## 7. Signalement

- Suivre le processus GitHub Issues du dépôt ; taguer `security`.
- Pour une vulnérabilité critique : email `security@wonkom.ai` (réponse sous
  72 h), PGP sur demande.
- `SECURITY.md` est le point d'entrée unique de la procédure ; un
  `.well-known/security.txt` doit être publié avec l'hôte de prod.
