#!/usr/bin/env bash
# =============================================================================
# CLOISON STACK-7 — SBOM (syft) + scan (grype/trivy) des images.
#
# Génère le SBOM SPDX JSON de chaque image construite puis la scanne.
# Politique : toute vulnérabilité HIGH/CRITICAL fait échouer le script
# (même règle que la CI, .github/workflows/ci.yml).
#
# Pré-requis (installés une fois sur wonkom.ai) :
#   curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b /usr/local/bin
#   curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh -s -- -b /usr/local/bin
#   # trivy (optionnel, doublon défense en profondeur) :
#   # curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh | sh -s -- -b /usr/local/bin
#
# Usage :
#   # 1. Construire les images (depuis la racine du dépôt) :
#   docker build -f deploy/Dockerfile.proxy  -t ghcr.io/coucagog/cloison-proxy:edge    .
#   docker build -f deploy/Dockerfile.control -t ghcr.io/coucagog/cloison-control:latest .
#   docker build -f deploy/Dockerfile.detect -t ghcr.io/coucagog/cloison-detect:latest .
#
#   # 2. Générer SBOM + scans (défaut : les 3 images ci-dessus) :
#   deploy/sbom.sh
#   # ou sur des images/tags spécifiques :
#   IMAGES="ghcr.io/coucagog/cloison-proxy:edge ghcr.io/coucagog/cloison-detect:latest" deploy/sbom.sh
#
# Sorties : $OUT_DIR/<nom-image>/<image>.spdx.json, grype.json, trivy.json,
#           et un résumé $OUT_DIR/report.txt.
# =============================================================================
set -euo pipefail

# --- Configuration -------------------------------------------------------------
OUT_DIR="${OUT_DIR:-sbom}"
SEVERITY_CUTOFF="${SEVERITY_CUTOFF:-medium}"     # grype : échec >= medium
TRIVY_SEVERITY="${TRIVY_SEVERITY:-HIGH,CRITICAL}" # trivy : échec >= HIGH
FAIL_ON="${FAIL_ON:-1}"                           # 0 = rapport seulement (audit)

DEFAULT_IMAGES=(
  "ghcr.io/coucagog/cloison-proxy:edge"
  "ghcr.io/coucagog/cloison-control:latest"
  "ghcr.io/coucagog/cloison-detect:latest"
)
IMAGES="${IMAGES:-}"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERREUR: '$1' introuvable dans PATH — voir Pré-requis en tête de script." >&2
    exit 2
  }
}

for tool in docker syft grype; do require "$tool"; done
if command -v trivy >/dev/null 2>&1; then HAVE_TRIVY=1; else HAVE_TRIVY=0; fi

if [ -z "$IMAGES" ]; then
  IMAGES="${DEFAULT_IMAGES[*]}"
fi

mkdir -p "$OUT_DIR"

overall=0
: > "$OUT_DIR/report.txt"

echo "==> SBOM + scan CLOISON (sortie: $OUT_DIR)"
echo

for image in $IMAGES; do
  echo "### $image"
  name="$(echo "$image" | tr '/:' '__')"
  dir="$OUT_DIR/$name"
  mkdir -p "$dir"

  # --- 1. SBOM SPDX (syft) -----------------------------------------------------
  echo "  [syft]  SBOM -> $dir/$name.spdx.json"
  syft scan "$image" -o spdx-json > "$dir/$name.spdx.json"

  # --- 2. Scan grype (échec si >= SEVERITY_CUTOFF quand FAIL_ON=1) --------------
  echo "  [grype] scan (seuil: $SEVERITY_CUTOFF) -> $dir/grype.json"
  if ! grype scan "$image" \
        --fail-on "$SEVERITY_CUTOFF" \
        -o json > "$dir/grype.json" 2> "$dir/grype.err"; then
    rc=$?
    echo "  [grype] VULNÉRABILITÉS >= $SEVERITY_CUTOFF détectées (rc=$rc) — voir $dir/grype.json"
    overall=1
  else
    echo "  [grype] OK (aucune vulnérabilité >= $SEVERITY_CUTOFF)"
  fi

  # --- 3. Scan trivy (défense en profondeur, si présent) ------------------------
  if [ "$HAVE_TRIVY" = "1" ]; then
    echo "  [trivy] scan (sévérités: $TRIVY_SEVERITY) -> $dir/trivy.json"
    if ! trivy image --quiet --scanners vuln \
          --severity "$TRIVY_SEVERITY" \
          --format json --exit-code 1 \
          "$image" > "$dir/trivy.json" 2> "$dir/trivy.err"; then
      rc=$?
      echo "  [trivy] VULNÉRABILITÉS >= $TRIVY_SEVERITY détectées (rc=$rc) — voir $dir/trivy.json"
      overall=1
    else
      echo "  [trivy] OK (aucune vulnérabilité >= $TRIVY_SEVERITY)"
    fi
  fi

  # Résumé par image.
  {
    echo "### $image"
    echo "  SBOM : $dir/$name.spdx.json"
    if [ -f "$dir/grype.json" ]; then
      echo "  grype: $(grep -o '"vulnerability"' "$dir/grype.json" 2>/dev/null | wc -l) matches (fichier: $dir/grype.json)"
    fi
  } >> "$OUT_DIR/report.txt"

  echo
done

echo "==> Résumé : $OUT_DIR/report.txt"
if [ "$overall" = "1" ]; then
  echo "==> ÉCHEC : au moins une image a des vulnérabilités >= seuil (grype/trivy)."
  if [ "$FAIL_ON" = "1" ]; then
    exit 1
  fi
  echo "==> (FAIL_ON=0 : rapport seulement, sortie 0)"
fi
exit "$overall"
