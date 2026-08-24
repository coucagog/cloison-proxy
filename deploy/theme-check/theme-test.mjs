//! Harnais de vérification des thèmes sombre/clair des pages CLOISON
//! (design system de référence, STACK-N0 §0).
//!
//! Teste, avec un DOM simulé en Node (aucun navigateur requis) :
//!   1. la bascule de thème (4 scénarios OS×état : transitions data-theme et
//!      aria-pressed correctes, 1er clic effectif en OS sombre) ;
//!   2. la cohérence CSS : aucune couleur hex en dur hors variables (sauf
//!      #fff sur fonds variables et #000 en mask-image), jeux de variables
//!      clair/sombre avec les mêmes clés colorimétriques ;
//!   3. les remaps SVG : toute couleur en dur dans les <svg> a un remap
//!      [fill=...]/[stroke=...] (adaptation au thème sombre).
//!
//! Pages testées :
//!   - deploy/journal-html/index.html (journal.wonkom.ai — dans le repo) ;
//!   - la topologie de référence (hors repo : Doc_REF/cloison-topologie_PII_V3.html)
//!     si présente (env CLOISON_TOPOLOGIE ou défaut ../../Doc_REF/…).
//!
//! Usage :  node deploy/theme-check/theme-test.mjs
//! Sortie : 0 = tout passe ; 1 = échec(s).

import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
// Racine du repo : deploy/theme-check → repo.
const ROOT = path.resolve(HERE, '../..');
const JOURNAL_PAGE = path.join(ROOT, 'deploy/journal-html/index.html');
const TOPOLOGIE_DEFAULT = path.resolve(HERE, '../../../Doc_REF/cloison-topologie_PII_V3.html');
const TOPOLOGIE = process.env.CLOISON_TOPOLOGIE || TOPOLOGIE_DEFAULT;

let failures = 0;

function makeDom(initialTheme, osDark) {
  const attrs = new Map();
  if (initialTheme) attrs.set('data-theme', initialTheme);
  let ariaPressed = 'false';
  const listeners = {};
  const el = {
    getAttribute: (k) => (k === 'data-theme' ? (attrs.get(k) ?? null) : null),
    setAttribute: (k, v) => {
      if (k === 'data-theme') attrs.set(k, v);
      else if (k === 'aria-pressed') ariaPressed = String(v);
    },
    addEventListener: (ev, fn) => { listeners[ev] = fn; },
    _click: () => listeners.click(),
  };
  return {
    doc: { documentElement: el, getElementById: () => el },
    win: { matchMedia: (q) => ({ matches: osDark && q.includes('dark') }) },
    el,
    aria: () => ariaPressed,
    theme: () => attrs.get('data-theme') ?? null,
  };
}

function check(name, cond) {
  if (cond) console.log(`  ✅ ${name}`);
  else { failures++; console.log(`  ❌ ${name}`); }
}

const NON_COLOR = ['--sans', '--mono', '--wrap'];

// Bascules testées : conscientes de l'OS (design system) — OS clair → dark,
// OS sombre → light au 1er clic (bascule réelle), puis alternance.
const SCENARIOS = [
  ['OS clair, sans état -> dark -> light', false, null, 'dark', 'light'],
  ['OS sombre, sans état -> light -> dark (bascule réelle au 1er clic)', true, null, 'light', 'dark'],
  ['OS sombre, dark -> light -> dark', true, 'dark', 'light', 'dark'],
  ['OS clair, light -> dark -> light', false, 'light', 'dark', 'light'],
];

function testToggle(html, getToggleCode, label) {
  const m = getToggleCode(html);
  if (!m) { console.log(`  ❌ ${label} : bloc de bascule introuvable`); failures++; return; }
  const prevMM = globalThis.matchMedia;
  for (const [name, osDark, initial, exp1, exp2] of SCENARIOS) {
    globalThis.matchMedia = (q) => ({ matches: osDark && q.includes('dark') });
    const { doc, win, el, aria, theme } = makeDom(initial, osDark);
    const btn = m(doc, win);
    btn._click(); const t1 = theme(), a1 = aria();
    btn._click(); const t2 = theme(), a2 = aria();
    const ok = t1 === exp1 && a1 === String(exp1 === 'dark') && t2 === exp2 && a2 === String(exp2 === 'dark');
    check(`${label} — ${name} (${t1}->${t2}, aria ${a1}->${a2})`, ok);
  }
  globalThis.matchMedia = prevMM;
}

function testCss(html, label) {
  const css = html.match(/<style>[\s\S]*?<\/style>/);
  if (!css) { console.log(`  ❌ ${label} : <style> introuvable`); failures++; return; }
  const blocks = [...css[0].matchAll(
    /(?:@media\(prefers-color-scheme:dark\)\{)?:root(?:\[data-theme="dark"\])?(?::not\(\[data-theme\]\))?\{[^{}]*\}(?:\})?/g,
  )].map((x) => x[0]);
  let rest = css[0];
  for (const b of blocks) rest = rest.replace(b, '');
  const hardHex = [...new Set([...rest.matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((x) => x[0].toLowerCase()))];
  // #fff = texte sur fonds variables ; #000 = mask-image (gradient alpha, thème-indépendant).
  const bad = hardHex.filter((h) => h !== '#fff' && h !== '#000');
  check(`${label} — couleurs hex en dur hors variables : ${bad.length === 0 ? 'aucune' : bad.join(',')}`, bad.length === 0);

  const lightBlock = blocks.find((b) => b.includes('--ink:#191C27'));
  const darkBlock = blocks.find((b) => b.includes('--ink:#E7E9EF'));
  if (lightBlock && darkBlock) {
    const k1 = [...lightBlock.matchAll(/--[\w-]+/g)].map((x) => x[0]).filter((k) => !NON_COLOR.includes(k)).sort();
    const k2 = [...darkBlock.matchAll(/--[\w-]+/g)].map((x) => x[0]).filter((k) => !NON_COLOR.includes(k)).sort();
    check(`${label} — jeux de variables clair/sombre identiques (${k1.length} clés colorimétriques)`,
      JSON.stringify(k1) === JSON.stringify(k2));
  } else { console.log(`  ❌ ${label} : blocs de variables introuvables`); failures++; }
}

function testSvgRemaps(html, label) {
  const svgColors = [...html.matchAll(/(?:fill|stroke)="#([0-9a-fA-F]{6})"/g)].map((x) => `#${x[1].toLowerCase()}`);
  const remaps = [...html.matchAll(/\[(?:fill|stroke)="#([0-9a-fA-F]{6})"\]/g)].map((x) => `#${x[1].toLowerCase()}`);
  const unremapped = [...new Set(svgColors)].filter((c) => !remaps.includes(c));
  check(`${label} — couleurs SVG en dur sans remap : ${unremapped.length === 0 ? 'aucune' : unremapped.join(',')}`,
    unremapped.length === 0);
}

// ---------- 1. Page journal (dans le repo) ----------
console.log(`=== 1. ${path.relative(ROOT, JOURNAL_PAGE)} ===`);
{
  const html = readFileSync(JOURNAL_PAGE, 'utf8');
  testToggle(html, (h) => {
    const m = h.match(/const themeBtn = document\.getElementById\('themeBtn'\);[\s\S]*?themeBtn\.setAttribute\('aria-pressed', next === 'dark'\);\s*\}\);/);
    return m ? new Function('document', 'window', m[0] + '\nreturn themeBtn;') : null;
  }, 'journal');
  testCss(html, 'journal');
}

// ---------- 2. Topologie de référence (hors repo, si présente) ----------
console.log(`=== 2. ${path.relative(ROOT, TOPOLOGIE)} ${existsSync(TOPOLOGIE) ? '' : '(absente — ignorée)'} ===`);
if (existsSync(TOPOLOGIE)) {
  const html = readFileSync(TOPOLOGIE, 'utf8');
  testToggle(html, (h) => {
    const start = h.indexOf('var root=document.documentElement');
    const iifeStart = h.lastIndexOf('(function(){', start);
    const end = h.indexOf('})();', start) + 4;
    if (start < 0 || iifeStart < 0 || end < 4) return null;
    const code = h.slice(iifeStart, end);
    return new Function('document', 'window', code + '\nreturn document.getElementById("themeBtn");');
  }, 'topologie');
  testSvgRemaps(html, 'topologie');
}

console.log(failures === 0 ? '\n✅ TOUS LES TESTS DE THÈME PASSENT' : `\n❌ ${failures} ÉCHEC(S)`);
process.exit(failures === 0 ? 0 : 1);
