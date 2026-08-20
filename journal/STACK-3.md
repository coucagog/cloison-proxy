# CLOISON — STACK-3 : cloison-proxy (Axum)

> Journal de développement — écrit au fil de l'eau. Gabarit : note technique §13.

## Objectif

Construire la **passerelle compatible OpenAI** : le produit visible. Elle s'intercale entre
une interface IA et un fournisseur LLM, tokenise les PII à l'aller (via cloison-core) et
restaure les jetons émis au retour — non-stream, stream SSE (buffer-and-scan), et tool-calls.

## Périmètre

**Dans :** `cloison-proxy` (Rust/Axum, `crates/cloison-proxy/`) — routes `/v1/chat/completions`
(non-stream + stream:true), `/v1/completions` (legacy), `/v1/models` ; auth clé composite ;
forwarding amont (reqwest) ; buffer-and-scan SSE ; restauration fail-loud ; tool-calls.

**Hors :** le plan de contrôle (STACK-5), le NER (STACK-6), le packaging (STACK-7).

## Décisions

1. **Clé composite** : `Authorization: Bearer mn_<jeton>.<cle_amont>` découpée sur le premier
   `.`. Le jeton identifie/autorise, la clé amont est transmise au fournisseur **uniquement
   via le header** — jamais en log, jamais en URL, `Debug` écrasé (aucune fuite).
2. **Registre par requête** : un `RequestEngine` neuf par requête (meilleur que le partage) —
   le registre d'émission naît et meurt avec la requête. Restauration : registre ET MAC exigés.
3. **Fail-loud non-stream** : une sentinelle tronquée (coupure max_tokens, `⟦` sans `⟧`)
   → **marqueur neutre** + compteur, jamais de jeton brut. Corrigé dans cloison-core
   (`extract_sentinel_positions` signale les ouvertures non fermées).
4. **Stream** : buffer-and-scan borné, n'émet que le texte confirmé, résout à la clôture ;
   `[DONE]` garanti. Timeout : `read_timeout` au lieu d'un timeout de corps global (qui
   couperait les streams longs en silence).
5. **Tool-calls** : tokenisation/restauration dans `function.arguments` (aller et retour) ;
   le premier chunk stream (`id`/`name`, arguments:"") est conservé (corrigé — il était perdu).

## Ce qui a été construit

- `src/main.rs`, `src/lib.rs`, `src/routes.rs` — serveur Axum, router 3 routes, body limit.
- `src/config.rs` — Config depuis env (port 8787, upstream_base_url, timeouts, key, salt).
- `src/errors.rs` — ProxyError (thiserror) + IntoResponse au format OpenAI.
- `src/auth.rs` — parsing clé composite, middleware 401 `invalid_api_key`, Debug écrasé.
- `src/openai.rs` — types serde chat/completions (messages, tool_calls, stream).
- `src/upstream.rs` — client reqwest (read_timeout), clé amont header-only, 502/504.
- `src/engine.rs` — pont cloison-core : RequestEngine par requête, tokenize/restore, fail-loud.
- `src/stream.rs` — buffer-and-scan SSE, sentinelles partielles, clôture, marqueur neutre.
- `src/handlers.rs` — chat_completions, completions_legacy, models, métriques.
- `tests/e2e.rs` — 11 scénarios contre un LLM mock (echo) : roundtrip non-stream, stream
  découpé, troncature→[REDACTED], tool-calls (stream compris), 401, jeton forgé, legacy, models.

## Comment lancer / tester

```bash
cd cloison && source ~/.cargo/env
cargo test -p cloison-proxy          # unit + e2e contre LLM mock
cargo clippy -p cloison-proxy -- -D warnings
# E2E réel (OpenRouter) : CLOISON_AUDIT_MODE=0 CLOISON_UPSTREAM_BASE_URL=… cargo run -p cloison-proxy
```

## Résultats

- **Compilation** : `cargo check -p cloison-proxy` OK.
- **Tests** : 17/17 verts (6 unit + 11 e2e). Core : 55/55 inchangés.
- **Clippy** : `-D warnings` → 0 erreur, 0 warning (proxy + core).
- **Revue QA indépendante** : verdict GO conditionnel — **3 failles P0 identifiées et
  corrigées** :
  1. Tool-calls en stream : le premier chunk (id/name, arguments:"") était supprimé → corrigé
     + test e2e dédié ajouté.
  2. Sentinelle tronquée en non-stream passée brute → corrigé dans cloison-core (ouvertures
     non fermées signalées → marqueur neutre).
  3. Timeout global reqwest coupant les streams → `read_timeout` (borné par lecture, pas par
     corps).

## Invariants de sécurité vérifiés

1. **Aucun secret en URL ni en log** : clé amont header-only, Debug écrasé, testé (scénario
   e2e : pas de clé dans les logs).
2. **Restaurer uniquement ce qu'on a émis** : registre par requête + MAC, testé (jeton forgé
   → marqueur neutre).
3. **Fail-loud** : troncature → `[REDACTED]` + compteur, jamais de jeton brut (testé).
4. **Tool-calls inclus** : function.arguments tokenisé/restauré (testé non-stream + stream).
5. **Stream sans fuite** : sentinelle coupée entre chunks jamais émise (testé découpe
   volontaire).

## Questions ouvertes / dette

- Le proxy forwarde les réponses de shape inconnue telles quelles (P1 QA : fail-loud
  contourné sur les champs inconnus). Acceptable v1, à durcir.
- Rougeoiement par champ entier (pas par jeton) : plus sûr, moins fin — à réévaluer.
- Scénario « aucune clé en log » non automatisé (tracing-test absent) : ajouter en STACK-7.

## Porte de sortie

- [x] Proxy e2e contre LLM mock : non-stream, stream, tool-calls.
- [x] 3 failles P0 QA corrigées et testées.
- [x] clippy -D warnings : zéro erreur.
- [ ] E2E contre un LLM réel (configuré via OpenRouter/DeepSeek) : à faire en STACK-7 (rodage).

## Prochaine étape

**STACK-4 — Mode Audit** : observation seule (détection + comptage sans masquage), rapport de
conformité, reçus signés (0 texte), seuils k-anonymat, pipeline corpus séparé documenté.
C'est le **premier produit livrable** de la séquence.
