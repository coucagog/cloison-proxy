# theme-check — vérification des thèmes sombre/clair

Harnais de vérification (Node, sans navigateur) des bascules de thème et de la
cohérence CSS/SVG des pages CLOISON (design system de référence, STACK-N0 §0).

```bash
node deploy/theme-check/theme-test.mjs
# sortie 0 = tout passe ; 1 = échec(s)
```

Ce qui est vérifié :

1. **Bascule de thème** (DOM simulé, 4 scénarios OS×état) : transitions
   `data-theme` et `aria-pressed` correctes ; **1er clic effectif en OS
   sombre** (bascule consciente de la préférence OS, alignée sur le design
   system).
2. **Cohérence CSS** : aucune couleur hex en dur hors variables (sauf `#fff`
   sur fonds variables et `#000` en mask-image) ; jeux de variables
   clair/sombre avec les **mêmes clés colorimétriques** (`--sans/--mono/--wrap`
   exclus : non colorimétriques, hérités).
3. **Remaps SVG** (page topologie) : toute couleur en dur dans les `<svg>` a
   un remap `[fill=...]`/`[stroke=...]` (adaptation au thème sombre).

Pages testées :

- `deploy/journal-html/index.html` (journal.wonkom.ai — dans le repo) ;
- la topologie de référence, hors repo (machine locale) :
  `Doc_REF/cloison-topologie_PII_V3.html` — chemin surchargeable par
  `CLOISON_TOPOLOGIE` ; absente → étape ignorée (warning).

Corrections STACK-N0 associées : bascule de la page journal alignée sur le
design system (`currentTheme()` OS-aware) ; remaps SVG manquants ajoutés dans
la topologie (`#AD3B3B` danger, `#CFD3DC` line) ; `matchMedia` →
`window.matchMedia` (robustesse). Voir `journal/STACK-N0.md` §0.
