# CLOISON — Guide client

> Brancher votre interface IA sur `api.wonkom.ai` : deux champs, aucune
> modification de code. Vos données personnelles n'atteignent **jamais** le
> modèle en clair, et nous ne les voyons pas — c'est vérifiable
> (journal public + code ouvert).

## 1. En deux champs

| Champ | Valeur |
|---|---|
| **Base URL** | `https://api.wonkom.ai/v1` |
| **Clé** | `mn_<jeton>.<cle_amont_du_client>` (clé composite fournie à l'onboarding) |

Configuration dans votre interface IA (Open WebUI, LibreChat, bolt.diy,
agents type Hermes, ou tout client compatible OpenAI) :

- **Open WebUI** : *Settings → Connections → OpenAI API* → Base URL
  `https://api.wonkom.ai/v1`, clé API = la clé composite.
- **LibreChat** : endpoint OpenAI personnalisé, même couple.
- **CLI / curl** :

```bash
curl https://api.wonkom.ai/v1/chat/completions \
  -H "Authorization: Bearer mn_<jeton>.<cle_amont>" \
  -H "Content-Type: application/json" \
  -d '{"model":"openai/gpt-4o-mini","messages":[{"role":"user",
       "content":"Bonjour, je m'appelle Aminata Diop, mon téléphone est le 77 123 45 67."}]}'
```

## 2. Ce qui se passe (promesse, architecture)

```
Interface IA ──▶ CLOISON edge (api.wonkom.ai) ──▶ Fournisseur LLM
                 pseudonymise la PII en ⟦…⟧        ne reçoit QUE des jetons
                 restaure les vraies valeurs ◀─── réponse
```

- **À l'aller** : noms, téléphones, emails, CNI, lieux… sont remplacés par
  des jetons `⟦…⟧` **avant** d'atteindre le fournisseur. Le modèle ne voit
  jamais la donnée personnelle en clair.
- **Au retour** : les vraies valeurs sont restaurées dans la réponse que
  vous recevez — la conversation reste naturelle et cohérente (les jetons
  déterministes gardent la coréférence : « Aminata » reste « Aminata »).
- **Nous ne voyons rien** : le cloud (plan de contrôle) ne manipule que des
  **compteurs k-anonymes signés**, jamais de texte.

## 3. FAQ confidentialité

**Q : Vos serveurs voient-ils mes données ?**
Non. Le moteur de pseudonymisation tourne à la frontière (`edge`) : le texte
clair y est traité en mémoire, puis seul le texte tokenisé part chez le
fournisseur. Le plan de contrôle ne stocke que des hash de jetons et des
compteurs agrégés (jamais de contenu, jamais de compteur isolé < seuil k).

**Q : Comment vérifier que vous ne lisez pas ?**
Par construction et par audit :
- **Journal public** : `https://journal.wonkom.ai` — chaque fenêtre d'audit
  y est enregistrée (compteurs k-anonymes, chaîne de hachage, signatures
  Ed25519). La page vérifie la chaîne **dans votre navigateur** (WASM) :
  aucune confiance requise.
- **Code ouvert** : la passerelle et les composants vérifiables sont
  publiés (`github.com/coucagog/cloison-*`, licences Apache-2.0 / AGPL-3.0
  pour la passerelle) — auditez, faites auditer, comparez.
- **Rapport de conformité** : en mode audit (observe-only), nous pouvons
  produire un rapport k-anonyme signé, présentable à votre DPO/auditeur.

**Q : Que se passe-t-il si la restauration échoue (jeton tronqué, coupé) ?**
Le proxy échoue **bruyamment** : il émet un marqueur neutre (`[REDACTED]`)
et incrémente un compteur — jamais de jeton brut, jamais de mauvaise valeur
en silence. Le compteur est visible dans le rapport de conformité.

**Q : Vos modèles comprennent-ils le français d'Afrique de l'Ouest ?**
Oui — c'est notre spécialité (noms sénégalais, formats de CNI, toponymie,
numéros +221). Le détecteur combine un moteur déterministe (regex +
gazetteers + validations Luhn) et un NER transformer entraîné sur
MasakhaNER (afroxlmr), validé par un benchmark public contre une baseline
Presidio forte (verdict GO : PERSON 0.94, LOC 0.84, macro 0.95).

**Q : Et les limites ?**
- Les **quasi-identifiants** (« le patient de 42 ans opéré le 3 mars à
  Ziguinchor ») peuvent ré-identifier sans nom : nous les **signalons**
  (jauge), nous ne prétendons pas les résoudre.
- Un poste **compromis** (malware local) sort du périmètre N0 : le coffre
  et les clés vivent chez vous.
- Le **modèle peut inventer** une PII sans jeton (problème de grounding,
  non résoluble par un proxy) — signalé comme recherche ouverte.

## 4. Périmètre de la promesse (niveaux de cloisonnement)

| Niveau | Où tourne le moteur | Ce que l'opérateur lit |
|---|---|---|
| **N0 local** | votre poste | rien (cible v1 de la promesse absolue) |
| **N1 site** | votre serveur | seulement vous, chez vous |
| **N3 hébergé** | chez l'éditeur (api.wonkom.ai) | entrée de gamme ; le clair ne quitte pas l'edge, le cloud ne voit que des compteurs |

## 5. Liens

- Journal public + vérification : `https://journal.wonkom.ai`
- Code ouvert : `github.com/coucagog/cloison-*`
- Documentation technique : `docs/` du dépôt (architecture, menaces, sécurité).

## 6. Rapport de conformité (mode audit)

Sur demande (mode audit observe-only activé pour votre tenant), le rapport
`GET /v1/audit/report` fournit, par période (`hourly|daily|weekly|all`) :
compteurs k-anonymes par type de PII détectée, restaurations incomplètes,
sorties bloquées, jauges quasi-id — **signés** (Ed25519, vérifiables avec le
code public) et **présentables** (aucun texte, aucune valeur isolée < k).

---

*CLOISON — proxy de confidentialité PII compatible OpenAI. La promesse est
vérifiable, pas contractuelle : auditez le code, surveillez le journal.*
