# Illustrations CLOISON — assets de marque

> Jeu d'illustrations vectorielles (SVG) de la marque CLOISON, à utiliser dans
> les interfaces générées (mobile/desktop) et les pages de la documentation
> publique (`docs.wonkom.ai`). Direction : **confiance, raffinement,
> professionnel** (voir `DESIGN.md`). Palette officielle clair/sombre.

## Catalogue

| Fichier | Rôle | Usage conseillé |
|---|---|---|
| `brand-mark.svg` | **Symbole de marque** : le cloison ⟦⟧ protégeant la donnée + wordmark. | Header, page d'accueil, splash d'app, pied de page. |
| `hero-flow.svg` | **Héro flux** : interface IA → CLOISON edge → fournisseur LLM, aller (PII) / retour (restauration). | Hero de landing, page « Comment ça marche », onglet produit. |
| `vault.svg` | **Coffre chiffré** (AES-256-GCM, clé HKDF, fail-loud). | Section « Sécurité / chez vous », écran N0, page coffre. |
| `journal-proof.svg` | **Journal de transparence** : chaîne de blocs Ed25519 + vérification WASM + compteurs k-anonymes. | Page « Vérification / preuve », état du journal. |
| `open-core.svg` | **Open-core** : fenêtre de code source ouverte + badge « vérifiable ». | Page « Code ouvert / licences », ethos. |
| `pattern-mark.svg` | **Motif de marque** répétable (sentinelles ⟦⟧, basse opacité). | Fond de section, transitions, surfaces décoratives. |

## Couleurs utilisées (tokens)

- **Jeton** `#0B7A85` / `#37B5C0` (dark) — identité, masquage, accent.
- **Edge** `#1E7A4D` / `#3FB878` (dark) — confiance, succès, vérifié.
- **Clair** `#A9640C` / `#E2A24C` (dark) — attention, donnée protégée.
- **Ink** `#191C27` / `#E7E9EF` · **Paper** `#E7E9EE` / `#0E1119` · **Panel** `#fff` / `#191F2C` — neutres.
- **Danger** `#AD3B3B` / `#E06A63` — erreurs (vault : badge fail-loud).

## Usage dans les interfaces générées

- Inline ou `<img src=".../illustrations/hero-flow.svg">` selon le contexte.
- Le **pattern-mark** se répète en `<image>`/CSS `background` avec
  `currentColor` basse opacité (0.04–0.08) pour rester discret.
- Toujours en **SVG** (pas de PNG) : net à l'échelle, compressible, redimensionnable.
- Convertir en WebP/AVIF pour les gros usages de l'open-core si besoin.
