# Design System — CLOISON (brand contract)

> Category: Trusted & Professional
> Proxy de confidentialité PII compatible OpenAI — le produit inspire confiance
> par la sobriété, la précision et la preuve. Design prêt à la vente : raffiné,
> élégant, professionnel — jamais gadget, jamais criard.
>
> **Illustrations de marque** : `design/illustrations/` (symbole, héro flux,
> coffre chiffré, journal de preuve, open-core, motif) — voir l'`INDEX.md`.
> Les interfaces générées DOIVENT embarquer ces assets SVG (jamais de PNG).

## 1. Visual Theme & Atmosphere

Interface de confiance : la sérénité d'un studio d'ingénierie, la clarté d'un
produit d'entreprise. Verticalité nette, respiration généreuse, accent discret
mais affirmé. Le logo mental : **calme, rigueur, maîtrise** — on confie ses
données personnelles à CLOISON, l'interface le mérite.

- **Visual style:** refined, minimal, professional, calm
- **Color stance:** surfaces neutres + accent sémantique maîtrisé
- **Design intent:** la hiérarchie d'information prime ; la couleur est un
  signal sémantique (confiance, masquage, attention), jamais une décoration.

## 2. Color

Palette officielle CLOISON (design system STACK-N0 §0) — clair / sombre via
variables `--token`.

**Clair (défaut) :**
- **Ink (texte principal):** `#191C27` — encre profonde, lecture longue.
- **Ink-soft:** `#535868` — texte secondaire.
- **Ink-faint:** `#8A8F9E` — méta-texte, indices.
- **Paper (fond):** `#E7E9EE` — fond général, neutre et doux.
- **Paper-2:** `#DEE1E8` — fond alterné.
- **Panel (surface):** `#FFFFFF` — cartes, surfaces élevées.
- **Line:** `#CFD3DC` · **Line-2:** `#E3E6EC` — séparateurs.
- **Jeton (accent teal — identité):** `#0B7A85` — le masquage, la confiance.
  `--jeton-bg:#E1F0F1` · `--jeton-line:#bfe0e3`.
- **Edge (vert — confiance/OK):** `#1E7A4D` — succès, états positifs.
  `--edge-deep:#1c4c36` · `--edge-bg:#E3F1E9` · `--edge-line:#c4e2d2`.
- **Clair (ambre — attention):** `#A9640C` — avertissements.
  `--clair-bg:#F6ECDD` · `--clair-line:#e4cfae`.
- **Danger:** `#AD3B3B` — erreurs. `--danger-bg:#F6E4E2` · `--danger-deep:#6f2b2b`.

**Sombre (auto / bascule) :**
- Ink `#E7E9EF` · Ink-soft `#A9AFBE` · Ink-faint `#79808F`
- Paper `#0E1119` · Paper-2 `#141824` · Panel `#191F2C`
- Line `#2A3141` · Line-2 `#232A38`
- Jeton `#37B5C0` (`--jeton-bg:#0E2A2E` · `--jeton-line:#1E4A50`)
- Edge `#3FB878` (`--edge-deep:#A7DCBD` · `--edge-bg:#12271B` · `--edge-line:#2A5540`)
- Clair `#E2A24C` · Danger `#E06A63` (`--danger-bg:#2E1817` · `--danger-deep:#E8938B`)

- **Jeton (teal) = l'accent d'identité.** L'utiliser pour les CTA primaires, les
  liens actifs, l'état « pseudonymisation active ».
- **Edge (vert) = les états de succès/confiance.** Jamais pour un action peak ;
  il signifie « vérifié / en cours / restauré ».
- **Clair (ambre) = attention honnête** (limites, warn).
- Toujours respecter les paires `*-bg`/`*-line`/`*-deep` pour les blocs
  sémantiques (notes, cartes d'état) — jamais une teinte plate hors palette.

## 3. Typography

- **Family (UI):** `system-ui,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif`
  — sobre, lisible, natif sur chaque OS.
- **Mono (code/jetons, très identitaire):** `ui-monospace,"SF Mono","JetBrains Mono","Cascadia Code",Menlo,Consolas,monospace`
  — pour les sentinelles `⟦…⟧`, les clés, les hachages, le code.
- **Scale (clair/air):** 12 · 14 · 15.5 · 17 · 19 · 23 · 30 · 44 (px). Corps
  17 px / interlignage 1.62 — confortable, « éditorial pro ».
- **Weights:** 400 corps · 500 labels/strong · 700–800 titres (letter-spacing
  légèrement négatif sur les gros titres).
- En-tête/hero : titrage **bold, lisible, jamais décoratif** ; corps optimisé
  pour le scan et le contraste (AA sur les surfaces).

## 4. Spacing & Grid

- **Spacing scale:** 8pt baseline (`8 | 14 | 18 | 22 | 54 | 74`).
- Conteneur **`--wrap: 1060px`** — lisible, centré, généreux.
- `24px` d'air latéral sur mobile ; les sections respirent (`padding 54px`).
- Grille cohérente, pas d'offsets ad-hoc.

## 5. Layout & Composition

- Header sticky, translucide (backdrop blur) — la marque reste visible en
  scroll, sans écraser le contenu.
- Hiérarchie évidente : **eyebrow (mono, uppercase, letter-spacing) → titre →
  lede → action**.
- Cartes/panels avec **radii 12–14px**, bordure `1px`, ombre très douce
  (`0 14px 34px -30px var(--shadow)`) — élévation discrète, jamais criarde.
- Blocs sémantiques avec la paire `*-bg`/`*-line` (note edge = confiance,
  warn = attention, panel = info).

## 6. Components

- **Boutons primaires :** fond `--edge` (vert confiance) ou `--jeton` (teal
  identité) selon le contexte ; texte blanc ; radius 9px ; hover opacity .85 ;
  focus-visible outline `2.5px --jeton`.
- **Boutons secondaires :** surface panel + bordure line ; texte ink.
- **Inputs :** `--field-bg`, bordure `--field-line`, focus outline `--jeton`
  (clairement visible — l'utilisateur doit savoir où il saisit).
- **Cards/status :** classe `.card.ok` (edge-bg, border edge-line, titre deep)
  vs `.card.ko` (danger) — le verdict d'état doit se lire au premier coup d'œil.
- **Pills/tags :** mono, uppercase, radius 999px, bordure fine — pour les
  labels de statut.

## 7. Motion & Interaction

- Transitions courtes et discrètes : **150–250ms**, easing stable, sur
  `background-color/border-color/color` — la réactivité sans flottement.
- États explicites pour **hover, focus-visible, active, disabled, loading**
  (jamais de bouton muet).
- Respecter `prefers-reduced-motion` (désactiver transitions/animation).

## 8. Voice & Brand

- Tone : **précis, confiant, honnête**. On parle de sécurité et de preuve —
  pas de jargon marketing vaporeux.
- Microcopy orienté action (« Vérifier », « Installer N0 », « Restaurer ») ;
  jamais de texte générique.
- Le mot-clé de la promesse : **vérifiable**. L'interface la rend tangible
  (journal, open-core, statut de masquage).
- Compatible a11y : contrastes AA, labels d'état lus par l'accessibilité.

## 9. Anti-patterns

- **Hors-palette :** aucune teinte hors tokens — toujours résoudre avec un
  token existant (préserver le contrat de marque).
- **Ne pas aplatir la hiérarchie :** pas de même taille/poids partout ; les
  titres portent la personnalité, le corps porte le scan.
- **Pas d'effets décoratifs** qui réduisent lisibilité ou accessibilité (pas de
  dégradés criards, pas d'ombres lourdes).
- **Ne pas mélanger les métaphores visuelles** dans la même interface.
- **Ne pas surcharger :** la sobriété est l'argument de vente — l'interface
  inspire confiance en étant calme.
