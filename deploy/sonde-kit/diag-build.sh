#!/usr/bin/env bash
# diag-build.sh — diagnostic lecture-seule du build d'image mania-cloison-edge
set -u
echo "=== processus build/docker ==="
ps -eo pid,etime,cmd | grep -Ei 'build-cloison|dockerd|docker build' | grep -v grep | head -12
echo
echo "=== log build (tail 25) ==="
tail -n 25 /tmp/build-edge.log
echo
echo "=== images mania-cloison ==="
docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' | grep mania-cloison || echo "(image absente)"
echo
echo "=== disque /tmp et / ==="
df -h /tmp | tail -1
df -h / | tail -1
