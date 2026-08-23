#!/usr/bin/env bash
# =============================================================================
# CLOISON — Contrôle quotidien d'expiration du certificat TLS (charte §12).
#
#   - Vérifie le certificat servi par api.wonkom.ai:443 (interrogation TLS).
#   - ALERTE (sortie 1 + message sur stderr) si < 14 jours restants (J-14) :
#     un renouvellement Caddy qui échoue doit déclencher une alerte AVANT
#     l'incident, pas après.
#   - Sortie 0 si le certificat expire au-delà du seuil (renouvellement OK).
#
# Installation (timer systemd, cf. deploy/cert-expiry.{service,timer}) :
#   sudo cp deploy/cert-expiry-check.sh /usr/local/bin/
#   sudo cp deploy/cert-expiry.service deploy/cert-expiry.timer /etc/systemd/system/
#   sudo systemctl daemon-reload && sudo systemctl enable --now cert-expiry.timer
#
# La sortie non-zéro du timer est visible dans `systemctl status
# cert-expiry.timer` et journald — brancher un canal d'alerte (mail/webhook)
# sur cet échec. Le script est ré-exécutable à la main :
#   bash deploy/cert-expiry-check.sh [domaine] [seuil_jours]
# =============================================================================
set -uo pipefail

DOMAIN="${1:-api.wonkom.ai}"
THRESHOLD_DAYS="${2:-14}"

# Date d'expiration du certificat servi par le domaine.
enddate=$(echo | timeout 10 openssl s_client -connect "$DOMAIN:443" -servername "$DOMAIN" 2>/dev/null \
  | openssl x509 -noout -enddate 2>/dev/null | cut -d= -f2-)

if [ -z "$enddate" ]; then
  echo "ERREUR: certificat introuvable pour $DOMAIN (connexion TLS impossible)" >&2
  exit 1
fi

expiry_epoch=$(date -d "$enddate" +%s)
now_epoch=$(date +%s)
remaining=$(( (expiry_epoch - now_epoch) / 86400 ))

echo "$(date -Is) $DOMAIN expire le $enddate — $remaining jour(s) restant(s)"

if [ "$remaining" -lt "$THRESHOLD_DAYS" ]; then
  echo "ALERTE J-$remaining : le certificat $DOMAIN expire dans moins de $THRESHOLD_DAYS jours — renouvellement Caddy à vérifier" >&2
  exit 1
fi

exit 0
