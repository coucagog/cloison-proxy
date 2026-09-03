#!/usr/bin/env bash
# packs-check.sh — état des packs du gabarit (lecture seule)
set -u
grep -H '^PII=' /opt/hermes/gabarit/packs/*.conf
echo '---'
ls -la /opt/hermes/gabarit/packs/*.bak-* 2>/dev/null | head -8
