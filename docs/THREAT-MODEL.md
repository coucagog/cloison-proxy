# CLOISON — Modèle de menace

> Fondation (STACK-0). À compléter à chaque STACK, jamais à alléger.
> Principe : traiter le poste, le LLM et l'opérateur comme trois adversaires potentiels distincts.

## Adversaires

| Adversaire | Ce qu'il peut faire | Ce qu'il voit | Protection |
|---|---|---|---|
| **Opérateur** (nous, plan de contrôle) | Lire la config, les compteurs, le journal | **Rien** en clair — compteurs agrégés k-anonymes seulement | Architecture aveugle : coffre au bord, 0 PII sur le cloud |
| **Fournisseur LLM** (OpenAI, Anthropic…) | Lire tout le trafic qu'il reçoit | **Uniquement des jetons** (jamais de PII, jamais de quasi-identifiants en clair) | Tokenisation aller, restauration retour, généralisation des faibles cardinalités |
| **Poste compromis** (malware, vol) | Lire le coffre local, la clé, la mémoire | **Tout** (le coffre et la clé y vivent) | Non protégé en N0 — honnêteté documentée, jamais de promesse contraire |
| **Tiers réseau** (entre poste et LLM) | Intercepter le trafic | Jetons uniquement (le clair ne sort jamais du poste en N0) | TLS partout, aucune PII en clair sur le fil |
| **Modèle LLM malveillant / hallucinant** | Inventer une PII, reformater les jetons | Les jetons émis + le contexte | Registre d'émission par requête, somme de contrôle, fail-loud à la restauration |

## Niveaux de cloisonnement (ce que l'opérateur peut lire)

| Niveau | Où tourne le moteur | Ce que l'opérateur lit | Rôle produit |
|---|---|---|---|
| **N0 local** | poste de l'utilisateur | **rien** | **cible v1** — promesse absolue (hors poste compromis) |
| **N1 site** | serveur du client | seul le client, chez lui | entreprise on-prem |
| **N2 enclave** | matériel scellé attesté | personne, et c'est prouvé | hors périmètre v1 |
| **N3 hébergé** | chez l'éditeur | l'éditeur lit le clair | entrée de gamme, jamais l'argument |

## Limites assumées (jamais masquées)

1. **Ré-identification par contexte sans nom** : « le patient de 42 ans opéré le 3 mars à
   Ziguinchor » → on **signale** une densité de quasi-identifiants (jauge), on ne prétend
   pas résoudre.
2. **PII inventée par le modèle** (hallucination sans jeton) : non résolvable par le proxy
   seul ; détecteur expérimental signalé, jamais présenté comme résolu.
3. **Poste compromis** : N0 ne protège pas contre malware/vol local. À écrire dans la
   promesse produit.

## Menaces spécifiques à vérifier à chaque STACK

- **STACK-2 (core)** : collision de jetons, restauration hors registre, fuite de jeton brut,
  déterminisme vs rotation des sels de session.
- **STACK-3 (proxy)** : secret en URL/query string, logs d'accès contenant Authorization,
  jeton coupé en deux chunks SSE, injection de faux jetons.
- **STACK-5 (contrôle/ledger)** : re-identification par agrégats fins (k-anonymat), 
  falsification du journal, clés Ed25519 compromises.
