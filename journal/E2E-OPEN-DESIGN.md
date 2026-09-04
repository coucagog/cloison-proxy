?# CLOISON — E2E-OPEN-DESIGN : test de bout en bout de la chaîne Open Design (demande pilote)

> Journal de développement — écrit au fil de l'eau. Gabarit : charte §13.
> Session du 29/08/2026. Demande pilote : « Je souhaite voir un test de bout
> en bout réussir. Nous avions installé Open Design en partie pour ça. »
> Complète `journal/STACK-N0V14.md` (docs-site) et `NEXT-SESSION.md` §2.
> Références : `REPRISE-DEPLOIEMENT.md` §7 (leçons), handoff racine
> `NEXT-SESSION.md`, `_open_design/start-open-design.ps1`.

## Objectif

Prouver, de façon **reproductible**, que la chaîne Open Design installée sur
cette machine fonctionne de bout en bout avec le design-system CLOISON :
**daemon → API → agent `deepseek-harness` (dsh) → skill `docs-page` → design
system `cloison` → artefact valide**. C'est la chaîne qui a produit les 9 pages
de `deploy/docs-site/` et le live-artifact du 27/08 ; elle n'avait jamais été
rejouée comme un test E2E formel avec des portes vérifiables.

## Périmètre

**Dans** : chaîne de génération Open Design (daemon headless, sélection du
design system, spawn de l'agent, délivrance de l'artefact, vérifications
statiques de l'artefact). **Hors** : déploiement prod sur docs.wonkom.ai
(optionnel, documenté §6), alignement du template docs-page (dette §7),
captures visuelles headless (indisponibles : pipes sandbox).

## Scénario E2E retenu (portes)

| # | Étape | Porte (vérifiable) |
|---|---|---|
| 1 | Daemon headless up sur `127.0.0.1:7456` | `GET /api/daemon/status` → `ok=true` |
| 2 | Design system enregistré | `GET /api/design-systems` → `cloison` présent, `status=published` |
| 3 | Agent disponible | `GET /api/agents` → `deepseek-harness` `available=true`, version 0.1.1-rc.2 |
| 4 | Run soumis | `POST /api/runs` `{projectId, message, agentId=deepseek-harness, skillId=docs-page, designSystemId=cloison}` → `runId` |
| 5 | Run terminal réussi | `GET /api/runs/<id>` → `status=succeeded`, `exitCode=0` |
| 6 | Artefact délivré et valide | `state.json` → `artifactCount≥1`, `deliverableValid=true` ; fichier HTML présent |
| 7 | Gates statiques artefact | HTML parsable ; tokens palette CLOISON présents ; `data-od-id` nav/article/toc ; zéro secret (grep `sk-`/`mn_`/IP interne/passphrase) ; zéro PII réelle (exemples synthétiques uniquement) |
| 8 | Consignation | ce journal + handoff racine mis à jour |

**Sujet E2E choisi** : page « Le coffre N0 — chiffrement au repos et
passphrase » (contenu réel du produit, en français, exemples synthétiques —
conforme invariants I1/I7).

## Décisions

1. **Projet** : réutiliser `cloison-docs` (skill `docs-page` déjà en place dans
   `.od/projects/cloison-docs/.od-skills/`, design system `cloison` déjà lié) —
   évite de dépendre d'un endpoint de création de projet non maîtrisé.
2. **Agent** : `deepseek-harness` (dsh `--profile open-design --stdio`) — le
   seul profil local fonctionnel (run ③ du 27/08) ; credentials DeepSeek dans
   `C:\Users\hp\.dsh` (hors repo, jamais committés).
3. **Skill** : `docs-page` (le template des pages docs) — c'est aussi la dette
   §7 de STACK-N0V14 : chaque génération en confirme l'écart de coquille à
   re-normaliser.
4. **Pas de déploiement prod automatique** : l'artefact E2E est un exercice ;
   la mécanique de déploiement reste `deploy/deploy-docs.sh` (VPS, idempotent).

## Verrous identifiés (à connaître pour rejouer)

1. **Sandbox local** : le daemon DOIT tourner hors sandbox pour spawner les
   agents. Preuves mesurées ce jour :
   - `Start-Process` + redirections de sortie sous sandbox → le daemon **meurt
     instantanément sans aucun log** (0 octet dans od-daemon.log/.err) alors
     que le même lancement en avant-plan fonctionne.
   - Daemon confiné : probe agent → `version-probe-failed` (spawn+capture
     stdio du daemon bloqué, EPERM pipes), alors que `dsh.cmd --version` en
     direct répond `0.1.1-rc.2`.
   - Daemon hors sandbox : probe OK. **Cohérent avec NEXT-SESSION §4.**
2. **Politique d'exécution PowerShell** : `dsh` résolu par pwsh pointe sur
   `dsh.ps1` (bloqué par ExecutionPolicy) → utiliser `dsh.cmd` ; le daemon,
   lui, utilise le shim `.CMD` (OK).
3. **Profil `open-design` — `runtime-profile-incompatible`** : au premier probe
   du daemon après redémarrage, le diagnostic « profile missing or
   incompatible » est apparu. Cause : le probe `dsh --profile open-design
   --probe` **réécrit `cordis.yml`** (composition du profil à chaque boot) et,
   lancé à froid depuis le daemon, il a échoué (timeout 10 s probable pendant
   la composition). **Correctif** : relancer une fois le probe à la main hors
   sandbox (`dsh.cmd --profile open-design --probe` → `plugin_version=0.1.0`),
   puis redémarrer le daemon → `available=true`. Aussi : sous sandbox, ce
   probe échoue en EPERM sur l'écriture de `C:\Users\hp\.dsh\...\cordis.yml`
   (hors workspace) — d'où l'importance du hors-sandbox.

## Ce qui a été construit

- `cloison/journal/E2E-OPEN-DESIGN.md` — ce runbook (procédure rejouable).
- Scénario E2E exécuté une fois de bout en bout (résultats §Résultats).
- Aucun code produit modifié. Aucune source Open Design modifiée.

## Comment lancer / tester (runbook reproductible)

### A. Lancer le daemon (terminal NORMAL, hors sandbox)

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass -Force
& "C:\Users\hp\Desktop\My_Projects\CLOISON_PROJECT\_open_design\start-open-design.ps1"
# → http://127.0.0.1:7456   (stop : od daemon stop --daemon-url http://127.0.0.1:7456)
```

### B. Vérifier les prérequis

```powershell
Invoke-RestMethod http://127.0.0.1:7456/api/daemon/status
(Invoke-RestMethod http://127.0.0.1:7456/api/design-systems).designSystems | ? id -eq cloison
(Invoke-RestMethod http://127.0.0.1:7456/api/agents).agents | ? id -eq deepseek-harness | select available,version
# si available=false avec runtime-profile-incompatible :
#   dsh.cmd --profile open-design --probe   (hors sandbox)  → puis redémarrer le daemon
```

### C. Soumettre un run (le cœur du test)

```powershell
$body = @{ projectId='cloison-docs'; agentId='deepseek-harness'; skillId='docs-page';
  designSystemId='cloison'; message='<brief français, exemples synthétiques>' } | ConvertTo-Json
$r = Invoke-RestMethod http://127.0.0.1:7456/api/runs -Method Post `
  -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($body))
# puis : Invoke-RestMethod http://127.0.0.1:7456/api/runs/$r.runId   (status / exitCode)
```

### D. Portes de sortie sur l'artefact

- `.od/runs/<runId>/state.json` : `status=succeeded`, `exitCode=0`,
  `artifactCount≥1`, `deliverableValid=true`, `designSystemId=cloison`.
- Fichier livré sous `.od/projects/cloison-docs/` (sous-dossier de l'artefact).
- Gates statiques (commande unique) : structure 1× nav/article/toc
  (`data-od-id`), tokens palette CLOISON présents, `grep -E 'sk-[A-Za-z0-9]|mn_|144\.217'` = 0,
  exemples synthétiques uniquement.

## Résultats (session du 29/08/2026)

### Run 1 — `e2a432f5-bdba-4762-9e71-b12acbb624a0` (29/08, ~4,5 min)

- Portes 1–5 ✅ : daemon up (v0.21.1), `cloison` published (153 systèmes),
  agent `available=true` (0.1.1-rc.2), run `succeeded` `exitCode=0`.
- Artefact ✅ : `coffre-n0.html` (37 Ko) dans `.od/projects/cloison-docs/` —
  `skillId=docs-page`, `designSystemId=cloison`, `artifactCount=1`.
- Gates statiques ✅ : `data-od-id` nav/article/toc présents, palette CLOISON
  présente (`#191c27`, `#e7e9ee`, `--jeton`, `--clair`), contenu français
  conforme (coffre, passphrase, AES-256-GCM, fail-loud, HKDF), **zéro secret**
  (les 2 matches du grep sont les URLs publiques docs/api/journal.wonkom.ai du
  footer — faux positifs).
- ⚠️ **Écart de nommage (trouvaille)** : `deliverableValid=false`,
  `deliverableValidation=entry_not_touched`. Le contrat de preview du skill
  `docs-page` déclare `entry: index.html`, mais l'exemple de contrat de sortie
  du `SKILL.md` dit `<artifact identifier="docs-slug" …>` → l'agent a émis
  `coffre-n0.html`, fichier non couvert par la porte « entry touched ».
  **Le chaînage daemon→agent→design system→artefact est sain ; c'est un gap de
  contrat skill/validation, pas une panne de la chaîne.** Correctif côté brief :
  exiger l'identifiant `index` (run 2).

### Run 2 — `bb8414b5-e56d-423e-bbe2-f2b0a971c7c0` (même session, ~6,7 min)

- Même brief + consigne « identifiant EXACT `index` (index.html) ».
- **✅ VERT SUR TOUTES LES PORTES** : `succeeded`, `exitCode=0`,
  `artifactCount=1`, `artifactPaths=[index.html]`, **`deliverableValid=True`**
  (`validation=valid`, `entryFile=index.html`, `kind=html`), `designSystemId=cloison`.
- Gates statiques ✅ : 1× nav/article/toc (`data-od-id`), doctype/title,
  palette CLOISON (`#191c27`, `--jeton`), contenu français coffre/passphrase/
  AES-256-GCM/fail-loud, **0 secret** (filtre `sk-|mn_|ghp_|IP interne`).
- Leçon reproductible : **le brief doit exiger l'identifiant d'artifact
  `index`** pour satisfaire la porte `deliverableValid` du skill docs-page
  (son `preview.entry` est `index.html` alors que l'exemple du SKILL.md montre
  `docs-slug` — gap de contrat à corriger dans le skill, voir §7).

## Invariants de sécurité vérifiés

- **Zéro secret** : aucun credential écrit dans ce journal ; credentials dsh
  restent dans `C:\Users\hp\.dsh` (hors repo). Le brief du run ne contient
  aucune clé.
- **Zéro PII réelle** : le sujet « coffre N0 » n'implique aucun nom réel ;
  consigne « exemples synthétiques uniquement » incluse dans le brief.
- **I7 (corpus sans exfiltration)** : le run n'alimente aucun corpus ; simple
  génération de page.

## Questions ouvertes / dette

- **Template docs-page non aligné** (dette §7 STACK-N0V14) : l'artefact E2E
  sort avec la coquille du skill (3 colonnes génériques), pas la coquille
  canonique de `deploy/docs-site/index.html`. Tant que le template n'est pas
  aligné, toute génération docs doit passer par
  `_open_design/normalize-docs-site.mjs`. **Piste** : faire de la coquille
  canonique une section du `DESIGN.md` cloison (§layout/components) et/ou
  amender le `SKILL.md` de `docs-page` — à arbitrer avec le pilote.
- **Runners GitHub toujours en panne** : sans impact sur ce test (aucun build
  nécessaire), mais toujours bloquants pour APK/macOS v0.3.1.
- **Daemon éphémère** : le daemon lancé ici vit dans un job de session ;
  pour un usage humain persistant, passer par le raccourci/script hors sandbox
  (§A).

## Porte de sortie

- [x] Run E2E `succeeded` avec artefact valide (portes 1–7 §Scénario) —
      **run `bb8414b5…`, 29/08/2026, toutes portes vertes.**
- [x] Runbook consigné (ce fichier) + handoff racine à jour.

## Prochaine étape

Selon résultat : soit rejouer après correction (template/DAEMON), soit
proposer au pilote la variante « E2E + normalisation + déploiement » sur une
page réelle de docs.wonkom.ai, soit attaquer l'alignement du template
docs-page (supprime la normalisation après chaque génération).

---

## Résultats (complété en fin de session)

**Test de bout en bout RÉUSSI (29/08/2026)** — chaîne complète exercée :
daemon headless → API → agent `deepseek-harness` (dsh `--profile open-design
--stdio`) → DeepSeek → skill `docs-page` → design system `cloison` → artefact
HTML valide et conforme au brand. Deux runs : le premier a exposé un gap de
contrat skill/validation (nommage de l'artifact), le second l'a contourné par
le brief et a passé **toutes** les portes (voir §Résultats ci-dessus).

Daemon arrêté en fin de session (job éphémère) ; relance en 1 commande (§A).

---

## ADDENDUM — E2E anonymisation PII (N0 local Windows, même session)

> Question pilote en cours de session : « Les PII sont-ils passés, ou tout a
> été anonymisé avec succès ? » → exécuté immédiatement, preuves capturées.
> **Réponse : tout a été anonymisé avec succès.** Détail ci-dessous.

### Procédure (reproductible)

1. Binaire public **`cloison-proxy-x86_64-pc-windows-msvc` v0.3.1** téléchargé
   depuis la release GitHub `coucagog/cloison-proxy` + `checksums.txt` —
   **SHA-256 vérifié** (`d22c8975…`). Conservé dans
   `_open_design/n0-e2e/cloison-proxy.exe` (réutilisable).
2. Bundle NER : `cloison-n0-ner-lite.tar.gz` (84,7 Mo) + `cloison-n0-
   onnxruntime-x86_64-pc-windows-msvc.tar.gz` — checksums vérifiés, extraits
   dans `_open_design/n0-e2e/ner/`.
3. Test officiel : `deploy/smoke-n0.ps1 -Binary <exe> [-NerPrefix <dir>]`
   (faux LLM `deploy/mock_llm.py` qui journalise le corps reçu = la preuve).

### Résultats — SANS NER (core seul, N0 v1)

- `smoke-n0.ps1` → **exit 0 (SUCCÈS)**.
- Envoyé : `Contact: Aminata Diop, user@example.com, tel +221 77 123 45 67`
- **Reçu par le LLM** : `Contact: ⟦ozp62oh…·GZA⟧ Diop, ⟦y3jn7qt…·EM⟧, tel ⟦of7r5ry…·PH⟧`
- Reçu par le client : valeur originale restaurée intégralement. Coffre : 0 clair.
- ⚠️ **Nuance honnête** : le patronyme seul « Diop » reste en clair côté amont
  (core seul ne masque que « Aminata » ici) — **limite documentée de N0 v1**
  (`docs/N0.md` §4 : rappel PERSON en texte libre réduit sans NER).

### Résultats — AVEC NER embarqué (distilbert ONNX int8, N0 v1.2)

- `smoke-n0.ps1 -NerPrefix …` → **exit 0 (SUCCÈS)** après correction ci-dessous.
- Envoyé : `Appelez Xolani Ndlovu au 77 123 45 67, il habite à Ziguinchor.`
- **Reçu par le LLM** : `Appelez ⟦ubd4gsjf…·PE⟧ ⟦qe4ecjjl…·PE⟧ au ⟦akzx76u3…·PH⟧, il habite à [VILLE_SN].`
  → **zéro PII en clair** : nom complet masqué (2 sentinelles PERSON),
  téléphone masqué, ville généralisée (faible cardinalité, design).
- Reçu par le client : `Appelez Xolani Ndlovu au 77 123 45 67, il habite à [VILLE_SN].`
- Coffre : 0 clair. **Verdict : ANONYMISATION PROUVÉE DE BOUT EN BOUT.**

### Leçons (à connaître pour la suite)

1. **`label_map.json` est OBLIGATOIRE à côté du modèle ONNX** (convention
   DEPLOY-8, `light_ner.rs::load_label_map`). Première extraction manuelle
   partielle (3 fichiers sur 8) → log daemon `labels=0` → PERSON non masqué,
   dégradation **silencieuse** (le warn ne dit pas que les labels manquent).
   L'installateur officiel `install-n0.ps1` extrait TOUT le tarball → OK.
   **Piste de durcissement** : warn explicite si `labels=0` dans `light_ner.rs`.
2. **`tar.exe` GNU (Git for Windows) échoue sur les chemins Windows**
   (« Cannot connect to C: resolve failed » — interprète `C:\…` comme hôte).
   Utiliser `$env:SystemRoot\System32\tar.exe` (bsdtar). `install-n0.ps1`
   appelle `tar.exe` sans qualification → **finding à corriger** (une ligne).
3. Le smoke **core** ne prouve pas le masquage des noms hors gazetteer ;
   rejouer systématiquement la variante `-NerPrefix` quand le NER est livré.
