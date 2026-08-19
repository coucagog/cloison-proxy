# CLOISON — Invariants de sécurité

> Non négociables. Chaque PR doit démontrer qu'elle ne les viole pas.
> Tout doute → choisis l'option qui rend une fuite **impossible par construction**.

## Invariants

1. **Zéro PII sur le plan de contrôle.** Aucune donnée personnelle en clair n'est jamais
   persistée sur le cloud, dans les logs, ni dans le journal de transparence.
2. **Le coffre reste au bord.** La table `jeton ↔ valeur` ne vit que côté client
   (N0/N1/N2). Le cloud aveugle ne la voit jamais.
3. **Restaurer uniquement ce qu'on a émis.** On ne restaure qu'un jeton présent dans le
   registre d'émission de la requête en cours ET dont la somme de contrôle est valide.
   Jamais une chaîne qui « ressemble » à un jeton.
4. **Échouer bruyamment.** Un échec de restauration bloque ou émet un marqueur neutre +
   incrémente un compteur ; il n'émet jamais un jeton brut ni une mauvaise valeur en
   silence. La substitution par faux réaliste exige une vérification de restauration
   complète avant émission.
5. **Généraliser, pas tokeniser les faibles cardinalités.** Sexe, ville, jour, tranche
   d'âge, quasi-identifiants → généralisation/suppression (k-anonymat). Jamais un simple
   jeton.
6. **Aucun secret dans les URLs ni les logs d'accès.** Le secret vit dans le champ
   « clé » (clé composite, jeton rotatif). Le reverse-proxy ne journalise jamais
   `Authorization` ni les query strings.
7. **Corpus sans exfiltration.** Le corpus se bâtit à partir de sources publiques +
   synthétique + opt-in explicite. La donnée réelle d'un client ne sert jamais à nourrir
   le corpus sans opt-in.
8. **Déterminisme dans la session, rotation entre sessions.**
   `HMAC(clé_locataire ‖ sel_session, valeur)`. Le sel de session est la rotation.
9. **Preuve sans texte.** Toute action de masquage est prouvable via des compteurs
   signés, sans jamais exposer de contenu. Seuils d'agrégation (k-anonymat) respectés.
10. **Tool-calls inclus.** Tokeniser/restaurer aussi dans
    `tool_calls[].function.arguments` et les résultats d'outils.
11. **Périmètre honnête.** Quasi-identifiants et PII hallucinée sont signalés, jamais
    prétendus résolus.
12. **Le benchmark précède le produit.** La porte GO/NO-GO de STACK-1 conditionne la suite.

## Règles de développement

- **Justesse avant vitesse** : un chemin lent et correct bat un chemin rapide qui fuit.
- **Adversarial par défaut** : menace-modéliser chaque fonctionnalité avant de l'écrire.
- **Fail-loud, jamais fail-silent** : en cas de doute, on bloque ou on marque — on ne
  devine pas.
- **Zéro clair en log, zéro mapping en trace** : relire chaque `tracing::info!` comme un
  adversaire.
- **Données de test = synthétiques uniquement** : jamais de PII réelle, même en dev.

## Politique de divulgation

À rédiger avant la première publication publique (dette STACK-0, à traiter en STACK-7).
