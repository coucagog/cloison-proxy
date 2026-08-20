# CLOISON — deploy

Contenu opérationnel (STACK-3/7) :
- `Dockerfile.proxy` / `Dockerfile.control` / `Dockerfile.detect` : images
  multi-stage des trois rôles (edge 8787, control 8788, detect 8080/50051).
- `docker-compose.dev.yml` : stack de dev — seul le port 8787 (edge) est
  publié ; control/detect restent sur le réseau interne `cloison-net`.
- `helm/` : charte Kubernetes (Deployment/Service/PVC/Secret/Ingress, sondes
  opérationnelles par défaut).
- `e2e_reel.sh` + `mock_llm.py` : e2e anti-pass-through (masquage amont
  prouvé) + LLM réel — voir `docs/DEPLOY.md` §9.
- `sbom.sh` : SBOM (syft) + scans (grype/trivy) hors CI.
- `.env.example` : modèle de secrets locaux (aucune valeur réelle).
