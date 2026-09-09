# CLOISON N0 sur Omarchy — procédure d'installation durable

Version validée le 09/09/2026 sur ce poste (Omarchy 4.0.2, Arch Linux, x86_64).
Installation de référence faite à `~/.cloison` (v0.3.3.1 épinglée).

## 1. Installer (version épinglée — jamais `latest`)

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh) \
  --version v0.3.3.1
```

- Le script vérifie les checksums SHA-256 (échec bruyant si incohérent) et affiche
  **une seule fois** `CLOISON_TENANT_KEY_HEX` → le conserver précieusement.
- Répertoire : `~/.cloison/` (binaire + `ner/` modèle ONNX int8 + onnxruntime).

## 2. Configurer l'environnement du service

Créer `~/.cloison/daemon.env` (chmod 600) :

```bash
CLOISON_ROLE=edge
CLOISON_LISTEN_ADDR=127.0.0.1:8787
CLOISON_UPSTREAM_BASE_URL=<URL du fournisseur réel, ex. https://openrouter.ai/api/v1>
CLOISON_VAULT_PATH=$HOME/.cloison/vault.redb
CLOISON_VAULT_PASSPHRASE=<passphrase choisie — jamais stockée ailleurs>
# ou : CLOISON_VAULT_KEYCHAIN_SERVICE=cloison-n0  (keychain OS recommandé)
CLOISON_EXPECTED_ACCESS_TOKEN=mn_<jeton local>
CLOISON_TENANT_KEY_HEX=<clé affichée à l'installation>
CLOISON_NER_MODEL_ONNX=$HOME/.cloison/ner/model-int8.onnx
CLOISON_NER_TOKENIZER=$HOME/.cloison/ner/tokenizer.json
CLOISON_ONNX_LIB=$HOME/.cloison/ner/libonnxruntime.so
# CLOISON_AUDIT_MODE=1   # =1 observe-only (détecte, ne transforme PAS) ; absent/0 = masquage actif
# CLOISON_SESSION_SALT_FILE=$HOME/.cloison/.salt
```

> ⚠️ Sémantique `CLOISON_AUDIT_MODE` vérifiée par l'expérience le 09/09 :
> `1` → **observe-only** (le texte part en clair, uniquement détecté/journalisé) ;
> `0`/absent → masquage actif. Attention : la FAQ publique prêtait à confusion.

## 3. Service systemd (user) pour un démarrage persistant

`~/.config/systemd/user/cloison-n0.service` :

```ini
[Unit]
Description=CLOISON N0 privacy proxy (pseudonymisation PII)
After=network.target

[Service]
Type=simple
EnvironmentFile=%h/.cloison/daemon.env
ExecStart=%h/.cloison/cloison-proxy
Restart=on-failure
RestartSec=3
# daemon dédié : ne pas brider le CPU (la détection NER est le goulot)
# CPUWeight=100

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now cloison-n0
systemctl --user status cloison-n0
```

Vérification : `curl http://127.0.0.1:8787/v1/models -H "Authorization: Bearer mn_<jeton>.sk-test"`.

## 4. Brancher un agent

- **Agent OpenAI-natif (Codex `wire_api=chat`, opencode)** — direct :
  `base_url = http://127.0.0.1:8787/v1`, clé `mn_<jeton>.<clé_amont>`.
- **Claude Code (Anthropic)** — non compatible directement (pas de `/v1/messages`) :
  passer par un adaptateur local (LiteLLM / claude-code-router) qui traduit
  Anthropic → OpenAI vers `http://127.0.0.1:8787/v1`.

## 5. Sauvegardes & précautions (sessions longues)

- Vault persistant, TTL par défaut **7 jours** ; un redémarrage du daemon ne perd
  rien si `vault.redb` + `.salt` + passphrase sont intacts.
- **Ne jamais** régénérer la tenant key / la clé composite en cours de session
  (nouveau sel ⇒ jetons antérieurs irrécupérables).
- Sauvegarder `vault.redb` + `.salt` ensemble, passphrase hors de l'agent.

## 6. Benchmark de référence

```bash
python3 cloison-poc/gen_bench.py          # génère bench/payload-{10k,50k,100k}.json
bash cloison-poc/bench-hote.sh            # mesure ~/cloison-poc/bench-resultats.txt
```

Mesures du 09/09 (poste Omarchy, sous sandbox 4 cœurs — indicative) :
| Contexte | Latence aller |
|---|---|
| ~10k tokens | ~18 s |
| ~50k tokens | ~89 s |
| ~100k tokens | ~194 s |

→ Scaling ≈ **2 s / 1 000 tokens** sur cette machine. Pour des contextes 100–200k,
prévoir un daemon dédié sans limite CPU et re-mesurer hors sandbox avant d'engager.

## 7. Désinstallation

```bash
systemctl --user disable --now cloison-n0
rm -rf ~/.cloison
```
