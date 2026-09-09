# Note de transmission — Session conjointe de re-test Omarchy (R2)

> **Statut : BROUILLON — à valider par le pilote avant envoi.**
> Destinataire : équipe d'évaluation (PoC Omarchy).
> Objet : session conjointe de re-test après la correction `reasoning_content`
> (**v0.3.3.2 publiée**), scénario, critères d'acceptation, preuves attendues.
> Fichiers joints : `protocole-R2-omarchy.md` (pas à pas + checklist) et
> `sonde-reasoning-omarchy.py` (sonde autonome exécutable chez vous).
> Rédigé le 10/09/2026.

---

## 1. Ce qui a changé côté CLOISON (v0.3.3.2, publiée le 09/09)

- **`reasoning_content` géré de bout en bout** :
  - aller : champ **connu et tokenisable** — le raisonnement rejoué entre les
    tours part **uniquement sous forme de jetons ⟦…⟧**, jamais en clair ;
  - retour : restauration des sentinelles **dans** `reasoning_content`,
    réponses non-stream **et** deltas SSE (sentinelle tronquée → `[REDACTED]`,
    jamais de jeton brut).
- **Seuil NER par défaut = 0,70** (aligné code + install + doc — votre choix
  manuel devient le défaut).
- **Docs** : manuel N0 public (`https://docs.wonkom.ai/manuel-n0.html`) et
  FAQ Q·9 (`CLOISON_AUDIT_MODE`, encodages exacts).
- Validation interne : portes de tests complètes vertes (dont 3 nouveaux tests
  reasoning) + re-sonde sur notre tenant réel (restaurations exactes, 0 clair
  fournisseur, 0 échec NER, 0 sentinelle brute).

## 2. Pré-requis côté Omarchy

1. **Mise à niveau** : `install-n0.sh --version v0.3.3.2` (jamais `latest`) —
   le coffre `vault.redb` et le sel `.salt` sont **conservés** par l'installeur.
2. Garder votre configuration : service systemd user `cloison-n0`,
   `127.0.0.1:8787`, clé composite `mn_omarchy.<clé>` (0600),
   `CLOISON_MAX_BODY_BYTES=8 MiB`. Le seuil NER 0,70 déjà posé reste correct
   (désormais le défaut).
3. opencode v1.18.30 + provider `cloison-deepseek` inchangés (cf.
   `install-cloison-omarchy.md`).

## 3. Scénario de la session (3 volets, ~1 h)

| Volet | Contenu | Support |
|---|---|---|
| **A — Sonde reasoning autonome** | roundtrip non-stream multi-tours + stream, PII synthétiques, vérifie seul l'essentiel de v0.3.3.2 | `sonde-reasoning-omarchy.py` (joint) |
| **B — opencode multi-tours « thinking »** | votre agent, 2+ tours sur une question impliquant un contact synthétique | `protocole-R2-omarchy.md` §3 |
| **C — Reprises P5/P6** | streaming SSE (chunks reasoning) + observe-only (`=1` **et** `=true`) | `protocole-R2-omarchy.md` §4 |

## 4. Critères d'acceptation

1. **Sonde A** : tous les contrôles PASS (tour 2 = 200 avec `reasoning_content`
   rejoué ; zéro sentinelle ⟦…⟧ en sortie, raisonnement inclus ; restaurations
   exactes).
2. **Volet B** : tours N≥2 = **200** dès lors que le client rejoue
   `reasoning_content` ; zéro sentinelle brute affichée par opencode.
3. **Volet C** : deltas `reasoning_content` reçus et **sans sentinelle** ;
   observe-only `=1` et `=true` → texte en clair côté fournisseur (conforme).

## 5. Preuves à collecter (nous les consoliderons dans le dossier REAL)

- sortie complète de `sonde-reasoning-omarchy.py` ;
- `journalctl --user -u cloison-n0` (période du test) — vérifié sans secret ;
- captures opencode (tours + erreurs éventuelles) ;
- toute divergence → horodatée + requête minimale de reproduction.

## 6. Limitations connues, dites d'avance

- **opencode ne rejoue toujours pas `reasoning_content`** (défaut SDK) : le 400
  multi-tours persistera **tant que le client ne rejoue pas le champ**. CLOISON
  le transporte désormais correctement ; le correctif durable est côté opencode
  (à leur signaler — body personnalisé en attendant). La sonde A prouve que la
  chaîne CLOISON fonctionne avec un client conforme.
- **Latence** : inchangée (NER fenêtré + DeepSeek direct) — le re-benchmark
  hors sandbox (`bench-hote.sh`) et les fenêtres d'inférence parallèles restent
  au plan.
- **Ville** : `[VILLE_SN]` par conception (généralisation irréversible), pas
  une régression.

## 7. Suite

Après R2 : F4 `/v1/responses` (Codex ≥ 0.153), puis F7 fenêtres parallèles,
F5 lexique (« Creuse »), F6 `/v1/messages` — selon l'arbitrage déjà acté.

---

**Pièces jointes** : `protocole-R2-omarchy.md`, `sonde-reasoning-omarchy.py`.
Références : `manuel-n0.html` (public), FAQ Q·9, note incident I1.
