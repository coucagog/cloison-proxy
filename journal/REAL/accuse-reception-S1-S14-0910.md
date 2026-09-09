# Accusé de réception — rapport final §10-11 (validation v0.3.3.2 + suggestions S1–S14)

> **Statut : BROUILLON — à valider par le pilote avant envoi.**
> Destinataire : équipe d'évaluation (PoC Omarchy).
> Objet : remerciements, tri de vos suggestions S1–S14, statut du correctif S4,
> proposition de feuille de route datée et de date pour la session conjointe.
> Rédigé le 10/09/2026.

---

## 1. Merci — le cycle est clos de votre côté

Votre §10 vaut acceptation : réinstallation propre en v0.3.3.2, `reasoning_content`
restauré (non-stream + SSE, **0 jeton complet résiduel**), **opencode multi-tours
avec outils sans plus aucun 400**. C'est exactement le critère d'acceptation F1
que nous avions fixé. Votre note sur les `⟦…⟧` tronqués (écrits par le modèle
lui-même, pas des fuites) est exacte : CLOISON ne laisse passer que ce que le
modèle génère spontanément hors registre — et ces formes ne sont jamais
résolues.

## 2. Tri de vos suggestions S1–S14

| # | Suggestion | Notre statut |
|---|---|---|
| S1 | `/v1/responses` (Codex) | **HAUTE, déjà arbitrée = F4.** Objectif : version majeure à périmètre Responses API. Votre §10.2 (Codex v0.153.4 bloqué) en fait un impératif marché. |
| S2 | Fenêtres d'inférence parallèles | **F7**, levier latence n°1 — PoC interne à chiffrer après F4. |
| S3 | Comptage brut local en observe-only | **Retenu.** Piste : compteurs bruts **locaux uniquement** (jamais publiés) pour l'audit initial — à cadrer côté plan de contrôle. |
| S4 | Défaut d'écoute `0.0.0.0:8787` | **Confirmé dans notre code, correctif PRÊT.** Voir §3. |
| S5 | Guide agents × reasoning | **Retenu** — matière déjà rédigée (note I1) ; sera publiée dans le manuel N0 (section « agents et modèles thinking »). |
| S6 | `CLOISON_MAX_BODY_BYTES` (1 MiB) | **Retenu** — à trancher entre défaut relevé (≥ 8 MiB) et documentation explicite ; nous préférons le défaut relevé + doc. |
| S7 | Lexique/whitelist (F5) | **Déjà arbitrée v0.3.4** — votre « Creuse » sera le cas de référence du test. |
| S8 | Référence env machine (`--help-env` / `print-config`) | **Retenu** (petit, haute valeur anti-dérive docs/code). |
| S9 | Encodages booléens | **Fait** — FAQ Q·9 publiée. |
| S10 | Sémantique des compteurs d'audit | **Retenu** — sera ajoutée au manuel N0 (`total_requests`, `masked_by_type` non exposé, `redacted` = projection k-anonyme, condition `publishable`). |
| S11 | Builds reproductibles + signatures | **Retenu** — dépend du retour des runners GitHub (en panne) ; cosign/attestations à brancher dès leur reprise. |
| S12 | `install-n0.sh` : version installée vs `latest` + pin | **Retenu** — affichage + recommandation d'épinglage. |
| S13 | Session conjointe « agent réel » | **Oui** — voir §4. |
| S14 | Registre clés amont / rotation chaude | **Roadmap N1**, priorité confirmée après F4/F7. |

## 3. S4 — statut précis

- **Confirmé** : `DEFAULT_LISTEN_ADDR = 0.0.0.0:8787` (code) — le daemon N0
  écoutait sur toutes les interfaces par défaut. Votre remarque est juste, merci.
- **Corrigé en code (à publier en v0.3.3.3)** : en mode N0 (`CLOISON_VAULT_PATH`
  posé), le défaut devient **`127.0.0.1:8787`** ; le mode edge (conteneurisé)
  conserve `0.0.0.0` ; `CLOISON_LISTEN_ADDR` explicite prime toujours. Tests
  ajoutés + manuel N0 mis à jour.
- **En attendant la release** : `CLOISON_LISTEN_ADDR=127.0.0.1:8787` explicite
  (c'est déjà votre configuration — rien à faire chez vous).

## 4. Session conjointe — proposition

Scénario (déjà transmis : `note-transmission-R2-omarchy.md` + protocole +
sonde) : **A** sonde reasoning autonome · **B** opencode multi-tours
« thinking » avec **streaming + outils** (on y tranche le quirk opencode
headless non-TTY) · **C** reprises P5/P6. Proposez-nous une date ; de notre
côté nous amenons la sonde et le protocole.

## 5. Feuille de route proposée (à dater avec vous)

1. **v0.3.3.3** (petit périmètre) : S4 (prêt), S6, S8, S12 ;
2. **v0.3.4** : F5 lexique (S7) + S3 + S10 ;
3. **v0.4.0** : F4 `/v1/responses` (S1) ;
4. F7 fenêtres parallèles (S2), puis S14 (N1).

Reste ouvert de notre côté : gazetteers publiables (Q17, engagement confirmé)
et arbitrage pilote Q19. Merci encore pour la rigueur de vos notes — elles
nous font gagner des cycles.
