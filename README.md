# CLOISON — proxy de confidentialité PII compatible OpenAI

> **Dépôt public (open-core) : la passerelle.**
> Documentation publique complète : **[https://docs.wonkom.ai](https://docs.wonkom.ai)**
> (promesse vérifiable, installation N0 ≤ 10 min, mobile, API, journal de
> transparence, open-core, FAQ + limites honnêtes).

CLOISON s'intercale entre votre interface IA et un fournisseur de LLM :
il **pseudonymise** la PII en jetons `⟦…⟧` avant l'envoi et **restaure** les
vraies valeurs dans la réponse. Le moteur descend chez vous ; le cloud n'est
qu'un plan de contrôle aveugle. La promesse est vérifiable (code ouvert +
journal de transparence) — aucune confiance demandée.

## Installation N0 (daemon local, ≤ 10 min)

Aucun prérequis (ni Rust, ni Python, ni torch) : binaire par OS + NER léger
embarqué + lib onnxruntime, checksums SHA-256 vérifiés.

**Linux / macOS :**

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh)
```

**Windows (PowerShell) :**

```powershell
powershell -ExecutionPolicy Bypass -File https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.ps1
```

Puis branchez votre interface IA sur `http://localhost:8787/v1`
(clé : `mn_<jeton_local>.<clé_amont>`). Guide complet : **docs.wonkom.ai/install-n0.html**.

## Ce dépôt contient

| Élément | Rôle |
|---|---|
| `src/`, `tests/` | la passerelle (crate Rust, AGPL-3.0) |
| `install-n0.sh` / `install-n0.ps1` | installation du daemon N0 (Linux/macOS/Windows) |
| `smoke-n0.ps1` | smoke test Windows du daemon |
| `provision_ner_lite.sh` | NER léger embarqué (téléchargement publié ; `--export` torch) |
| `mobile/` | apps mobiles v1 (Android + iOS) — WebView + moteur WASM, pseudonymisation in-app, coffre in-memory |

## Versions & binaires

Les releases (`v0.3.x`) portent les binaires par OS, le bundle NER (AFL-3.0)
et les libs onnxruntime épinglées : [releases](https://github.com/coucagog/cloison-proxy/releases).
Les autres composants open-core : `github.com/coucagog/cloison-*`
(core, ledger, verify, audit, control, cli, wasm, detect, bench).

## Licence

Ce dépôt est sous **AGPL-3.0** (la passerelle). Les composants vérifiables
sont Apache-2.0. Le corpus (gazetteers détaillés, jeux d'évaluation) est
propriétaire et n'est pas publié.
