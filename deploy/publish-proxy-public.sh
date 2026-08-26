#!/usr/bin/env bash
# =============================================================================
# CLOISON — Publication du dépôt public cloison-proxy (open-core).
#
# Reconstruit main = proxy (subtree crates/cloison-proxy) + fichiers racine
# PUBLICS (README, scripts d'install, mobile/) + LICENSE AGPL (leçon v0.2.5).
#
# POURQUOI ce script : la re-publication v0.3.1 a fait `push -f` de la branche
# split (crates/cloison-proxy SEUL) → les fichiers racine ajoutés à la main
# précédente (install-n0.sh, README…) ont été ÉCRASÉS (leçon v0.2.5 récidivée,
# constat 29/08 : install-n0.sh → 404 sur main). Ce script ré-applique la
# racine complète À CHAQUE publication.
#
# Usage (hôte VPS, repo à jour) : ./deploy/publish-proxy-public.sh
# Jeton GitHub lu dans ~/.git-credentials (jamais affiché). Idempotent.
# =============================================================================
set -euo pipefail

REPO=/home/debian/Cloison/cloison
OC=/home/debian/Cloison/cloison-open-core
PUB="https://github.com/coucagog/cloison-proxy.git"

echo "=== 0. repo à jour (source de vérité) ==="
cd "$REPO"
git rev-parse --short HEAD
git status --short | head -3

echo "=== 1. bundle du repo -> clone open-core ==="
cd "$REPO"
git bundle create /tmp/pub-proxy.bundle main >/dev/null 2>&1
cd "$OC"
git checkout -q main 2>/dev/null || true
git fetch /tmp/pub-proxy.bundle refs/heads/main:refs/remotes/bundle/main 2>/dev/null
git merge --ff-only refs/remotes/bundle/main
git update-ref -d refs/remotes/bundle/main
git log --oneline -1

echo "=== 2. re-split crates/cloison-proxy ==="
git branch -D pub5/proxy >/dev/null 2>&1 || true
git subtree split --prefix=crates/cloison-proxy --branch=pub5/proxy >/dev/null
git checkout -q pub5/proxy

echo "=== 3. overlay racine publique (LE cœur de la réparation) ==="
# README public (dépôt public — pas le README du monorepo)
git show main:deploy/public-repo/README.md > README.md
# Scripts d'installation grand public (les URLs du site docs les référencent)
git show main:deploy/install-n0.sh > install-n0.sh
chmod +x install-n0.sh
git show main:deploy/install-n0.ps1 > install-n0.ps1
git show main:deploy/smoke-n0.ps1 > smoke-n0.ps1
git show main:deploy/provision_ner_lite.sh > provision_ner_lite.sh
chmod +x provision_ner_lite.sh
# Cargo.lock épinglé (doctrine DEPLOY-7 — présent à la racine du workspace)
git show main:Cargo.lock > Cargo.lock
# LICENSE = AGPL-3.0 (leçon v0.2.5 : la passerelle est AGPL, pas Apache)
git show main:crates/cloison-proxy/LICENSE-AGPL-3.0 > LICENSE
# Mobile (Android + iOS) — publié dans CE dépôt (STACK-N0V15, dette ①)
git checkout main -- mobile
git add -A
git -c user.name="coucagog" -c user.email="coucagog@gmail.com" commit -q -m "chore(open-core): racine publique complète — README + scripts d'install + mobile/ (android+ios) + Cargo.lock + LICENSE AGPL (réparation écrasement v0.3.1, leçon v0.2.5)" || true
git log --oneline -1

echo "=== 4. push main (force — dépôt de publication open-core) ==="
git push -f -q "$PUB" HEAD:main
git checkout -q main

echo "=== 5. vérification des URLs publiques ==="
for u in README.md install-n0.sh install-n0.ps1 smoke-n0.ps1 provision_ner_lite.sh mobile/android/README.md mobile/ios/README.md; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "https://raw.githubusercontent.com/coucagog/cloison-proxy/main/$u")
  echo "  $u -> $code"
  [ "$code" = "200" ] || { echo "!! $u != 200" >&2; }
done

rm -f /tmp/pub-proxy.bundle
echo "=== PUBLICATION TERMINÉE ==="
