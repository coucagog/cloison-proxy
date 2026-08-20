# CLOISON — Modèle de menaces (STRIDE)

> Référence : code `crates/*`, `services/cloison-detect`, `docs/DATA-MODEL.md`,
> `docs/SECURITY.md` (invariants I*, I-A*, O*).

## 1. Flux de données

```
        PII claire (texte)                  sentinelles ⟦b32·TAG⟧        PII claire (texte)
Client ────────────────► edge ──────────────────────────► LLM réel ──(réponse tokenisée)──► edge ──► Client
        Bearer mn_<t>.<k>  │  tokenisation (cloison-core)         (restauration registre requête)
                           │  audit mode : comptage observe-only + reçu signé
                           ▼
                      detect (REST/gRPC) — spans uniquement, stateless
                           │
                           ▼
                      control (/admin/*) — hash de jetons, politiques
                           ▼
                      ledger (append-only, signatures Ed25519)
                           ▼
                      verify (public, WASM) — vérification hors-ligne
```

Périmètre des données sensibles : le **texte clair n'existe que dans le
processus edge** (et transitoirement dans detect). `control`, `ledger`,
`verify`, le corpus d'audit et le fournisseur ne voient **jamais** de texte.

## 2. STRIDE par composant

### 2.1 Edge (`cloison-proxy`)

| Catégorie | Menace | Traitement |
|---|---|---|
| Spoofing | clé composite forgée / rejouée | validation du préfixe `mn_` + temps constant (`CLOISON_EXPECTED_ACCESS_TOKEN`) ; 401 avant tout appel amont (I1, I3) |
| Tampering | corps modifié entre client et amont | TLS amont (rustls — **pré-requis STACK-7**, feature `rustls-tls`) ; MAC des jetons (HMAC-BLAKE3) |
| Repudiation | « le proxy n'a pas vu ma PII » | mode audit : reçu Ed25519 par requête (I-A3), rapport k-anonyme (I-A6) |
| Information disclosure | fuite PII vers le fournisseur / logs | tokenisation complète aller (I2), sentinelles sans clair ; logs sans texte (O2) ; clé amont jamais en log (I1) |
| DoS | gros corps, stream illimité | `CLOISON_MAX_BODY_BYTES` (1 MiB), tampon borné (I4), timeouts amont (5 s/30 s), keep-alive SSE |
| Elevation | exécution dans le conteneur | non-root 65532, read-only, tmpfs, cap_drop ALL (O3) — le garde-fou uid 0 de `main.rs` est un pré-requis STACK-7 |

### 2.2 Detect (`cloison-detect`)

| Catégorie | Menace | Traitement |
|---|---|---|
| Spoofing | spans forgés injectés au core | contrat strict : offsets validés contre `len(text)`, score ∈ [0,1], `extra="forbid"` sur les schémas REST |
| Information disclosure | fuite du texte reçu | stateless (aucune persistance), erreurs sans le texte d'entrée, docs auto désactivées |
| DoS | modèle lourd en boucle | budget temps (`CLOISON_BUDGET_SECONDS`), quarantaine après crash (`CLOISON_QUARANTINE_SECONDS`), chargement lazy |
| Elevation | exécution | uid 10001, read-only, /models volume (O3) |
| Tampering | paquets pip compromis | versions épinglées dans `requirements.txt`, SBOM+scan de l'image (O5) |

### 2.3 Control / Ledger

| Catégorie | Menace | Traitement |
|---|---|---|
| Spoofing | jeton `mn_` volé présenté au store | seul le **hash** SHA-256 est stocké ; `validate_token` compare les digests à temps constant ; rotation/revocation immédiate |
| Tampering | entrée du journal modifiée | `entry_hash = SHA-256(header 80 o)` + `prev_hash` chaîné + signature Ed25519 `verify_strict` ; toute altération casse la chaîne (O6) |
| Information disclosure | clair `mn_` persisté | le clair n'existe que dans la réponse d'émission ; `TokenIssued` affiché une fois |
| Repudiation | contrôle nie une entrée | signatures Ed25519 du contrôle vérifiables par `cloison-verify` (public) |
| DoS | écritures non terminales / trous de seq | `Ledger::append` refuse `seq != len`, `prev_hash` non lié, hash recomputé ≠ stocké, signature invalide |
| Elevation | compromission du store | Postgres cible derrière pool interne (feature `pg`) ; API admin jamais publiée hors réseau interne |

### 2.4 WASM client (`cloison-verify`)

| Catégorie | Menace | Traitement |
|---|---|---|
| Tampering | chaîne rejouée/altérée côté client | `verify_chain` revalide genèse, seq, prev_hash, entry_hash, ts, signatures |
| Spoofing | fausse clé publique de contrôle | la clé publique est fournie par le control-plane (canal de confiance documenté) |
| DoS | entrée JSON géante | parse borné par le moteur (entrées/sorties JSON string) ; aucun réseau |

### 2.5 Postgres (miroir externe, profil `db`)

- Credentials dev-only (`CLOISON_PG_PASSWORD`), réseau interne ;
- le registre **nominal** reste le ledger embarqué (`CLOISON_LEDGER_FILE`) —
  postgres est un export/audit externe, jamais la source de vérité ;
- `PostgresStore` (feature `pg`) : requêtes paramétrées (sqlx), jamais de
  concaténation SQL.

## 3. Frontières de confiance

1. **Réseau interne** compose/K8s (`internal: true`) : detect, control,
   postgres ne sont pas exposés à l'extérieur.
2. **Conteneur** : non-root, read-only, tmpfs — le processus n'a rien à
   écrire hors `/data` (volume) et `/tmp`.
3. **Texte clair** : uniquement à l'intérieur du processus edge (et
   transitoirement detect). Le corpus d'audit (reçus), le ledger et le
   fournisseur n'en contiennent jamais (I-A2, I-A5).
4. **Clé amont** : header `Authorization` de la requête amont uniquement.

## 4. Menaces traitées (cas concrets)

| Menace | Mitigation |
|---|---|
| Fuite PII vers le fournisseur (amont) | tokenisation complète aller (I2) — testée e2e contre mock et LLM réel |
| Fuite PII vers le client (aval) | restauration stricte : registre de requête + MAC ; sentinelle non résolue → `[REDACTED]` (I3) |
| Rejeu de reçus d'audit | reçu lié à la requête (`session_ref_hashed`, `ts_unix`, `policy_hash`) + signature ; k-anonymat des rapports (I-A6) |
| Falsification du registre | chaîne de hachage + signatures ; `verify_chain` détecte toute altération (O6) |
| Sidecar detect compromis | stateless (rien à exfiltrer de durable) ; spans validés ; exécution non-root read-only |
| Vol de la clé locataire | jamais en log/URL/corps (I1) ; `.env`/secrets K8s ; rotation du sel de session (I7) |
| Attaque sur le endpoint admin | API control sur réseau interne uniquement ; hash-only storage |
| Sentinelle forgée injectée en retour | MAC vérifié ; hors registre → bloquée (I2) ; fail-loud (I3) |

## 5. Risques résiduels assumés

1. **Canal de distribution de la clé publique du contrôle** : `cloison-verify`
   doit recevoir la clé de manière authentique (hors périmètre du code).
2. **Sidecar détecteur imparfait** : une PII non détectée n'est pas masquée
   (rappel < 100 %) — le benchmark STACK-1/STACK-7 fixe des seuils GO
   (PERSON/LOC ≥ 0.85 F1, CNI/MAIL/TEL ≥ 0.95) mais ne garantit pas l'absolu.
3. **Postgres de secours** : en mode miroir externe, sa configuration
   (mots de passe, réseau) relève de l'opérateur ; la charte Helm ne le
   déploie pas par défaut.
4. **Fournisseur LLM** : il reçoit des sentinelles mais voit la *structure*
   des messages ; le k-anonymat du rapport atténue, pas n'élimine, la
   corrélation statistique.
5. **WASM** : `@cloison/core` s'exécute dans le navigateur ; sa sandbox est
   celle du navigateur, pas celle du conteneur.
