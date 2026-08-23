# cloison-proxy

**La passerelle compatible OpenAI** de CLOISON — le produit visible. Elle s'intercale
entre une interface/agent IA (Open WebUI, bolt.diy, LibreChat, agents type Hermes) et un
fournisseur LLM : elle **pseudonymise** les PII (jetons `⟦…⟧`) avant l'envoi au modèle et
**restaure** les vraies valeurs dans la réponse — non-stream, stream SSE (buffer-and-scan)
et tool-calls inclus. Le moteur (détection, coffre) vit chez le client ; le cloud ne voit
jamais de donnée personnelle.

Fait partie du projet [CLOISON](https://github.com/coucagog/cloison).

## Points clés

- **Clé composite** : `Authorization: Bearer mn_<jeton>.<clé_amont>` — la clé amont est
  transmise au fournisseur uniquement via le header, jamais en log ni en URL.
- **Fail-loud** : une sentinelle tronquée → marqueur neutre + compteur, jamais de jeton brut.
- **Registre par requête** : on ne restaure qu'un jeton émis pendant CETTE requête ET à
  somme de contrôle valide (anti-hallucination).
- **Wiring C** : auth des jetons par hash auprès de
  [cloison-control](https://github.com/coucagog/cloison-control) (`/v1/control/verify`,
  fail-closed), ingest automatique des reçus d'audit (intervalle 60 s, curseur durable),
  long-poll `/v1/control/version` (rotation).
- **Sidecar NER** : consommation optionnelle de
  [cloison-detect](https://github.com/coucagog/cloison-detect) (`CLOISON_DETECT_URL`) pour
  fermer le fossé PERSON/LOC ouest-africain ; dégradation gracieuse si absent.

## Usage

```bash
cargo test -p cloison-proxy     # unit + e2e contre LLM mock (11 scénarios)
cargo clippy -- -D warnings
```

## Licence

**AGPL-3.0** — voir [LICENSE](LICENSE) (et [LICENSE-AGPL-3.0](LICENSE-AGPL-3.0)).
Décision open-core (charte §5.1) : la passerelle serveur est la seule à porter
l'AGPL-3.0, pour empêcher les forks hébergés fermés ; les composants vérifiables
restent Apache-2.0.
