# Previews CLOISON — maquettes de frontends (prêt à la vente)

> Maquettes HTML haute-fidélité qui appliquent le `DESIGN.md` (palette
> officielle, typographie, composants sémantiques) et les illustrations de
> `design/illustrations/`. À regarder dans un navigateur (ouvrir le .html) —
> pas de rendu headless possible dans la sandbox DSH (Edge headless bloqué).

| Fichier | Écran | Statut |
|---|---|---|
| `mobile-app.html` | **App mobile — écran de chat confidentiel** : header marque + statut, conversation pseudonymisée (jetons ⟦⟧, restauration, signal quasi-id), bandeau « chiffré chez vous », composer, footer confiance. Clair/sombre auto. | ✅ maquette |
| `desktop-dashboard.html` | **App desktop — dashboard confidentialité** : sidebar marque, hero « flux », KPIs (0 sentinelle, 3 types PII, 13 entrées journal), état du coffre, table du journal de transparence. Clair/sombre auto. | ✅ maquette |
| `landing.html` | **Landing page vendeuse** : hero « données jamais au modèle », bandeau confiance, produit (3 tuiles), preuve (3 étapes), CTA install. Clair/sombre auto, 100% offline. | ✅ maquette |
| `mobile-onboarding.html` | **App mobile — onboarding** : progression 3 dots, hero coffre, proposition de valeur, 3 features (coffre in-memory, restauration bornée, signal quasi-id), CTA + skip. Clair/sombre auto, 100% offline. | ✅ maquette |

## Comment en générer d'autres (via Open Design, une fois le daemon buildé)

Le daemon (`tools-dev run web`) consumera `cloison/DESIGN.md` +
`design/illustrations/` comme **brand contract** pour produire les variantes
(landing, dashboard, onboarding, deck) ou les écrans desktop. Voir
`_open_design/README.md` pour l'état d'installation.

## Tests de rendering

Ouvrir dans un navigateur : `start design/previews/mobile-app.html`
(Windows) ou double-clic. Basculer OS clair/sombre pour vérifier les deux
modes. Aucune dépendance externe, aucun réseau.
