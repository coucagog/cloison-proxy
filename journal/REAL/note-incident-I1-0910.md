# Note incident I1 — `reasoning_content` × modèles « thinking » (DeepSeek v4)

> **Statut : BROUILLON — à valider par le pilote avant envoi.**
> Destinataire : équipe d'évaluation (PoC Omarchy).
> Objet : synthèse factuelle de l'incident I1, correction engagée, plan de re-test.
> Rédigé le 10/09/2026, à partir de `consolidation-omarchy-2026-09-09.md` §2-I1
> et du code v0.3.3.1 vérifié dans le dépôt.
> **Mise à jour avant envoi : la correction est PUBLIÉE (release v0.3.3.2,
> 09/09 soir UTC) et re-validée en réel (re-sonde edge verte, 0 sentinelle
> brute, 0 échec NER).**

---

## 1. Ce qui s'est passé (rappel factuel)

- **Symptôme 1 — 400 multi-tours** : tour 1 OK (réponse restaurée), tours
  suivants → `400 "reasoning_content in the thinking mode must be passed back
  to the API"` (6 occurrences relevées dans `daemon-dbg.log`).
- **Symptôme 2 — sentinelle brute en sortie** : `⟦qeo6…·GZA⟧` visible dans le
  `reasoning_content` renvoyé par le modèle (`opencode-dbg2.txt` l.21-22/33).

## 2. Diagnostic, précisé après relecture du code

1. **Le 400 est d'abord un comportement du client.** opencode v1.18.30 (SDK
   openai-compatible) ne **stocke ni ne rejoue** `reasoning_content` entre les
   tours : la requête du tour N≥2 part **sans** ce champ, et DeepSeek la
   refuse — avec ou sans CLOISON. La transmission SSE de CLOISON (22 chunks
   vérifiés) est, elle, correcte.
2. **Correction d'honnêteté sur v0.3.3.1** : en relisant le code de la
   passerelle (`openai.rs`, `ChatMessage`), nous constatons que v0.3.3.1
   **écartait silencieusement** `reasoning_content` à l'aller (champ inconnu
   de serde, non conservé). Autrement dit : même un client qui rejouerait
   correctement le champ aurait reçu le même 400 à travers v0.3.3.1. Votre
   curl « avec replay → 200 » était donc nécessairement hors CLOISON ; la
   conclusion « proxy non en cause » reste vraie pour le 400, mais CLOISON
   **contribuait** au problème en ne sachant pas transporter ce champ.
3. **Le trou de restauration est avéré et assumé** : la restauration ne
   couvrait pas `reasoning_content` en réponse → une sentinelle re-mentionnée
   par le modèle dans son raisonnement ressortait brute côté client. C'est un
   défaut CLOISON, sans ambiguïté, corrigé en v0.3.3.2.

## 3. Correction engagée — v0.3.3.2 « reasoning_content géré »

- **Aller** : `reasoning_content` devient un champ **connu et tokenisable**
  (comme `content`) — le champ est désormais transmis au fournisseur, et
  uniquement sous forme de jetons ⟦…⟧ (plus aucun clair dans le raisonnement
  rejoué).
- **Retour** : restauration des sentinelles **dans** `reasoning_content` —
  non-stream (`choices[].message.reasoning_content`) **et** deltas SSE
  (réassemblage borné, mêmes règles fail-loud que `content` ; sentinelle
  tronquée → `[REDACTED]`, jamais de jeton brut).
- **Seuil NER** : le défaut passe à **0,70** (code + install + FAQ alignés —
  décision d'arbitrage, vos mesures sur du code plaidant pour 0,70).
- **Tests** : roundtrip reasoning non-stream + stream + sentinelle coupée dans
  un delta reasoning ; suites existantes inchangées.

## 4. Conséquence pour votre usage

- **Sur le 400 multi-tours** : avec v0.3.3.2, un client qui **rejoue**
  `reasoning_content` fonctionne en multi-tours sur DeepSeek thinking. Le
  défaut opencode (non-replay) reste de leur côté : à signaler à opencode ;
  en attendant, un body personnalisé ou un autre client qui rejoue le champ
  lève le 400.
- **Sur la sentinelle brute** : close avec v0.3.3.2 — zéro sentinelle en
  sortie, raisonnement inclus (critère d'acceptation).

## 5. Plan de re-test conjoint (après acceptation v0.3.3.2)

**La release v0.3.3.2 est publiée** (9 assets, checksums vérifiés) et
re-validée en réel sur notre tenant (re-sonde verte : restaurations exactes,
0 valeur claire côté fournisseur, 0 échec NER, 0 sentinelle brute dans les
logs). Pour votre poste : `install-n0.sh --version v0.3.3.2` (jamais
`latest`).

Scénario proposé : opencode multi-tours sur DeepSeek « thinking » à travers
N0 v0.3.3.2 — critères : tours N≥2 = 200 (avec un client qui rejoue
`reasoning_content`), zéro sentinelle brute en sortie (raisonnement inclus),
restaurations exactes, reprise des critères P5/P6 (streaming, observe-only).
Date : à votre convenance après la release.
