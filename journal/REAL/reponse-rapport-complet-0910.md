# Réponse au rapport complet — corrections livrées (v0.3.3.3) + protocole du dernier tour

> **Statut : BROUILLON — à valider par le pilote avant envoi.**
> Destinataire : équipe d'évaluation (PoC Omarchy).
> Objet : suite de votre rapport complet du 09/09 — vos constats §6.2/§6.3/§6.4
> et suggestions S15–S18 sont **corrigés en code**, publiés en **v0.3.3.3** ;
> protocole du dernier tour de tests.
> Rédigé le 10/09/2026.

---

## 0. Principe — compatibilité générique

CLOISON est une passerelle **OpenAI-compatible générique** : chaque correction
ci-dessous est un comportement de protocole (aller/retour chat/completions,
SSE, robustesse amont, restauration), valable pour **tout client**
OpenAI-compatible (opencode, Codex, LibreChat, Open WebUI, SDK maison…) et
**tout fournisseur** (DeepSeek, GLM, OpenRouter…). Votre environnement
Omarchy est notre terrain d'essai réel le plus exigeant — les preuves que
vous produisez sont des instanciations, jamais des adaptations à un client
particulier.

## 1. Ce qui est corrigé et publié (v0.3.3.3)

| # | Constat / suggestion | Correctif (générique) |
|---|---|---|
| **S17** | Modèle retire ⟦…⟧ et recopie l'intérieur des jetons (sonde 10/25) | **Restauration par intérieur de jeton nu** : tout corps de jeton (26 base32) recopié sans délimiteurs est restauré **ssi** il a été émis pour la requête en cours (registre + MAC — aucun faux positif possible). Actif par défaut, désactivable (`CLOISON_RESTORE_BARE_INNARDS=0`). |
| **S15** | 502/EOF sur raisonnements longs (165 s) | Timeout amont par défaut **5 min** + **un** réessai identique sur 502/503 ou corps tronqué (non-stream, jamais plus d'un — pas de retry aveugle). |
| **S16** | « Sénégal » masqué → réponse « Laos » | **Whitelist géo pays** (français + anglais, embarquée) : les noms de pays ne sont jamais masqués (défaut) ; + **classes désactivables** par env (`CLOISON_DISABLE_DETECTORS`, nom inconnu = refus de démarrer). |
| **S18** | Cas « sortie structurée » manquant | Tests e2e ajoutés : écho d'intérieurs nus (non-stream + stream, découpés au milieu des chunks) + retry 502/EOF — contre un **mock OpenAI-compatible générique**. |
| **S4** | Défaut d'écoute `0.0.0.0` | Défaut **`127.0.0.1:8787`** en mode N0 (edge conteneurisé inchangé ; env explicite prime). |
| **S6** | `MAX_BODY_BYTES` 1 MiB | Défaut **8 MiB**. |
| **S8** | Référence env | **`cloison-proxy --help-env`** : toutes les variables + défauts, imprimées par le binaire (source = code, anti-dérive). |
| **S12** | `latest` dangereux | `install-n0.sh` affiche la version installée vs la dernière release et recommande l'épinglage. |

Tests : core **108 + 17** (dont intérieurs nus, whitelist géo, toggle, noms
d'env), proxy **27 lib + 19 e2e** (dont les 4 nouveaux) — suite complète
verte localement et portes workspace sur notre serveur.

## 2. S3/S5/S10 — documentés

- **S10** : la sémantique du rapport d'audit est maintenant dans le manuel N0
  (`total_requests`, `redacted` = projection k-anonyme, `masked_by_type` non
  exposé, condition `publishable`).
- **S5** : section « Agents & modèles thinking » du manuel N0 (qui rejoue ou
  non `reasoning_content`, timeout 5 min, réessai EOF, intérieurs nus).
- **S3** (comptage brut local) : resté **piste produit** — pas livré en
  v0.3.3.3 ; proposé pour v0.3.4 avec le lexique (S7/F5).

## 3. Protocole du dernier tour de tests (proposition)

Reprenez la sonde **v2** (`sonde-reasoning-omarchy.py`, jointe — mêmes
variables d'env que la v1) après mise à niveau :

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh) --version v0.3.3.3
python3 sonde-reasoning-omarchy.py
```

| Volet | Scénario | Critère attendu |
|---|---|---|
| A | Tours reasoning non-stream + stream (PII synthétiques) | 200 partout, restaurations exactes, zéro sentinelle, `[DONE]` |
| **B** | **Sortie structurée** (une ligne / JSON : nom, email, tél, ville) | **Intérieurs nus restaurés** — le clair revient, plus aucun corps base32 ni délimiteur en sortie |
| **C** | **Géographie** (« capitale du Sénégal ») | « Sénégal » **non masqué** (whitelist) ; réponse naturelle ; ville SN → `[VILLE_SN]` par conception |
| D | Long raisonnement « thinking » | 200 (timeout 5 min) ; si EOF/502 ponctuel : **un** réessai visible en log |
| E | Observe-only `=1` et `=true` | texte en clair amont, reçus signés |
| F | opencode multi-tours **avec outils**, TTY si possible | plus aucun 400 ; quirk headless non-TTY à trancher (suspecté SDK) |

Preuves à collecter : sortie complète de la sonde, `journalctl --user -u
cloison-n0` (période du test), captures opencode — nous les consoliderons
dans le dossier REAL. **Critère global du dernier tour : 25/25 contrôles
sonde PASS** (la v1 en comptait 25 ; les échecs §6.3 deviennent des PASS).

## 4. Reste ouvert (à trancher ensemble)

1. **Feuille de route datée** : v0.3.3.3 (livrée) → v0.3.4 (F5 lexique + S3)
   → v0.4.0 (**F4 `/v1/responses`** — vos S1 et Codex) → F7 fenêtres
   parallèles → S14 (N1).
2. **S16 complémentaire** : whitelist géo = pays ; pour des cas métier plus
   fins (toponymes hors pays), le lexique v0.3.4 est le bon véhicule.
3. Gazetterers publiables (Q17 — engagement confirmé) ; Q19 (arbitrage
   pilote) ; quirk opencode headless → remonter à opencode si confirmé en TTY.

Merci pour la qualité de vos mesures — le §6.3 nous a fait gagner un cycle
entier.
