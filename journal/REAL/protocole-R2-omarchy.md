# Protocole R2 — Session conjointe Omarchy (pas à pas + checklist)

> Exécutant : équipe d'évaluation (Omarchy 4.0.2/Arch) avec nous en session.
> Données : **100 % synthétiques** (aucune PII réelle).
> Durée visée : ~1 h. En cas d'échec : noter l'étape, l'horodatage et la
> sortie brute — ne jamais relancer sans noter (reproductibilité).

---

## §0 — Mise à niveau vers v0.3.3.2

```bash
# 1. arrêt du daemon
systemctl --user stop cloison-n0
# 2. réinstallation ÉPINGLÉE (le coffre + le sel sont conservés)
bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh) \
  --version v0.3.3.2
# 3. redémarrage
systemctl --user start cloison-n0
# 4. vérification du boot (attendu : threshold=0.7, écoute 127.0.0.1:8787)
journalctl --user -u cloison-n0 --since "5 minutes ago" | tail -20
```

- [ ] Boot log : `threshold=0.7`
- [ ] Boot log : `cloison-proxy listening addr=127.0.0.1:8787`
- [ ] Service actif sans erreur

## §1 — Volet A : sonde reasoning autonome

```bash
# sur le poste Omarchy, clé composite via variable (jamais inline)
export CLOISON_N0_KEY="mn_omarchy.<votre clé>"   # identique au câblage actuel
python3 sonde-reasoning-omarchy.py
```

- [ ] Tour 1 non-stream : 200, PII restaurées (ville = `[VILLE_SN]` attendue)
- [ ] Tour 1 : `reasoning_content` reçu, **zéro sentinelle** dedans
- [ ] Tour 2 non-stream (reasoning rejoué) : **200**
- [ ] Tour 3 stream : `[DONE]`, PII restaurées, zéro ⟦/⟧ dans tous les deltas
- [ ] Bilan final de la sonde : `TOUS LES CONTRÔLES : PASS`

## §2 — Volet B : opencode multi-tours « thinking »

1. Config opencode inchangée (`cloison-deepseek`, modèle `deepseek-v4-flash`).
2. **Tour 1** : « Voici un contact fictif : Aminata Diop, aminata.diop@example.sn,
   téléphone +221 77 123 45 67, ville Ziguinchor. Raisonne dessus puis
   réponds : nom, email, téléphone, ville. »
3. **Tour 2** : « Et maintenant, redis exactement les quatre informations en
   une ligne. »
4. Noter : codes de retour visibles dans opencode, texte affiché, toute
   sentinelle `⟦…⟧` visible, toute mention du raisonnement.

- [ ] Tour 1 : réponse restaurée, aucune sentinelle visible
- [ ] Tour 2 : **200** si le client rejoue `reasoning_content` (sinon 400
      attendu — défaut opencode connu, voir note §6 ; le noter, ce n'est pas
      une régression CLOISON)
- [ ] Aucune sentinelle brute dans l'affichage opencode (raisonnement inclus)

## §3 — Volet C : reprises P5 (SSE) et P6 (observe-only)

**P5 — streaming** : demander en stream la même question que §2 tour 1 via
opencode (ou curl) ; compter les deltas `reasoning_content` reçus.

- [ ] Deltas `reasoning_content` reçus (ordre de grandeur attendu : ~20-25)
- [ ] Aucun delta ne contient ⟦ ou ⟧

**P6 — observe-only** : avec un fournisseur capturant (ou un mock local),
relancer deux fois en observe-only :

```bash
CLOISON_AUDIT_MODE=1   # puis, séparément : CLOISON_AUDIT_MODE=true
```

- [ ] `=1` → le fournisseur reçoit le texte **en clair** (rien n'est masqué)
- [ ] `=true` → **identique** (les deux encodages sont équivalents — FAQ Q·9)
- [ ] Retour en masquage actif : variable retirée + redémarrage, re-sonde §1

## §4 — Sortie de session

- [ ] Preuves collectées (sortie sonde, journalctl, captures opencode)
- [ ] Toute divergence notée (étape + heure + repro minimale)
- [ ] Statut global : ACCEPTÉ / ACCEPTÉ AVEC RÉSERVES / REFUSÉ (+ raisons)

> **Rollback éventuel** : `install-n0.sh --version v0.3.3.1` (le coffre et le
> sel survivent ; les jetons émis sous v0.3.3.2 restent restaurables).
