#!/usr/bin/env bash
# Deploy the distributed prover system to Kubernetes with optional dashboard

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DEPLOY_DIR="$PROJECT_ROOT/deploy/k8s"

# Parse arguments
DEPLOY_DASHBOARD=false
while [[ $# -gt 0 ]]; do
  case $1 in
    --with-dashboard)
      DEPLOY_DASHBOARD=true
      shift
      ;;
    --help)
      echo "Usage: $0 [options]"
      echo ""
      echo "Deploy Zelana Forge distributed ZK proof system to Kubernetes"
      echo ""
      echo "Options:"
      echo "  --with-dashboard    Also deploy the official Kubernetes Dashboard"
      echo "  --help             Show this help message"
      echo ""
      echo "Examples:"
      echo "  $0                    # Deploy Zelana Forge only"
      echo "  $0 --with-dashboard   # Deploy Zelana Forge + Dashboard"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      echo "Use --help for usage information"
      exit 1
      ;;
  esac
done

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

if [ "$DEPLOY_DASHBOARD" = true ]; then
    echo "==> Deploying Zelana Prover + Kubernetes Dashboard to Kubernetes"
else
    echo "==> Deploying Zelana Prover to Kubernetes"
fi
echo "Use --with-dashboard to also deploy the official Kubernetes Dashboard"
echo ""

# Check prerequisites
echo "Checking prerequisites..."
if ! command_exists kubectl; then
    echo -e "${RED}Error: kubectl not found. Please install kubectl.${NC}"
    exit 1
fi

if ! command_exists docker; then
    echo -e "${RED}Error: docker not found. Please install Docker.${NC}"
    exit 1
fi

echo -e "${GREEN} Prerequisites satisfied${NC}"
echo ""

# Check if kubectl is configured
if ! kubectl cluster-info &> /dev/null; then
    echo -e "${RED}Error: kubectl is not configured or cluster is not accessible.${NC}"
    echo "Please configure kubectl to point to your cluster."
    exit 1
fi

CLUSTER_INFO=$(kubectl cluster-info | head -n 1)
echo "Connected to: $CLUSTER_INFO"
echo ""

# Build Docker images
echo "==> Building Docker images..."
cd "$PROJECT_ROOT"  # Use project root as build context

echo "Building prover-node image..."
docker build -f deploy/docker/Dockerfile.node -t zelana/prover-node:latest .
echo -e "${GREEN} prover-node image built${NC}"

echo "Building prover-coordinator image..."
docker build -f deploy/docker/Dockerfile.coordinator -t zelana/prover-coordinator:latest .
echo -e "${GREEN} prover-coordinator image built${NC}"
echo ""

# Load images into local cluster if needed
if command_exists minikube && minikube status &> /dev/null; then
    echo "==> Loading images into minikube..."
    minikube image load zelana/prover-node:latest
    minikube image load zelana/prover-coordinator:latest
    echo -e "${GREEN} Images loaded into minikube${NC}"
    echo ""
elif command_exists kind && kind get clusters &> /dev/null; then
    CLUSTER_NAME=$(kind get clusters | head -n 1)
    echo "==> Loading images into kind cluster '$CLUSTER_NAME'..."
    kind load docker-image zelana/prover-node:latest --name "$CLUSTER_NAME"
    kind load docker-image zelana/prover-coordinator:latest --name "$CLUSTER_NAME"
    echo -e "${GREEN} Images loaded into kind${NC}"
    echo ""
fi

# Apply Kubernetes manifests
echo "==> Applying Kubernetes manifests..."
kubectl apply -k "$DEPLOY_DIR"
echo -e "${GREEN} Manifests applied${NC}"
echo ""

# Wait for resources to be ready
echo "==> Waiting for resources to be ready..."

echo "Waiting for namespace..."
kubectl wait --for=jsonpath='{.status.phase}'=Active namespace/zelana-prover --timeout=30s
echo -e "${GREEN} Namespace ready${NC}"

echo "Waiting for prover nodes..."
kubectl wait --for=condition=ready pod -l app=prover-node -n zelana-prover --timeout=120s
echo -e "${GREEN} Prover nodes ready${NC}"

echo "Waiting for coordinator..."
kubectl wait --for=condition=ready pod -l app=coordinator -n zelana-prover --timeout=120s
echo -e "${GREEN} Coordinator ready${NC}"
echo ""

# Show status
echo "==> Deployment Status"
echo ""
echo "Pods:"
kubectl get pods -n zelana-prover
echo ""
echo "Services:"
kubectl get services -n zelana-prover
echo ""

# Coordinator info
# Deploy dashboard if requested
if [ "$DEPLOY_DASHBOARD" = true ]; then
    echo ""
    echo -e "${BLUE}==> Deploying Kubernetes Dashboard...${NC}"
    echo ""

    # Deploy the official dashboard
    echo " Applying recommended dashboard manifests..."
    kubectl apply -f https://raw.githubusercontent.com/kubernetes/dashboard/v2.7.0/aio/deploy/recommended.yaml

    # Wait for deployment
    echo "⏳ Waiting for dashboard to be ready..."
    kubectl wait --for=condition=available --timeout=300s deployment/kubernetes-dashboard -n kubernetes-dashboard || {
        echo -e "${YELLOW}Warning: Dashboard deployment may still be in progress${NC}"
    }

    # Create admin service account
    echo " Creating admin service account..."
    kubectl apply -f - <<EOF
apiVersion: v1
kind: ServiceAccount
metadata:
  name: admin-user
  namespace: kubernetes-dashboard
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: admin-user
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: cluster-admin
subjects:
- kind: ServiceAccount
  name: admin-user
  namespace: kubernetes-dashboard
EOF

    echo -e "${GREEN} Dashboard deployed${NC}"
fi

echo ""
echo -e "${GREEN}==> Deployment complete!${NC}"
echo ""

# Show deployment status
echo "==> Zelana Forge Status"
echo ""
echo "Pods:"
kubectl get pods -n zelana-prover
echo ""
echo "Services:"
kubectl get services -n zelana-prover

if [ "$DEPLOY_DASHBOARD" = true ]; then
    echo ""
    echo "==> Dashboard Status"
    echo ""
    echo "Dashboard Pods:"
    kubectl get pods -n kubernetes-dashboard
    echo ""
    echo "Dashboard Services:"
    kubectl get services -n kubernetes-dashboard
fi

echo ""
echo " Access Instructions:"
echo ""
echo "Zelana Forge Coordinator:"
echo "  kubectl port-forward -n zelana-prover svc/coordinator 8080:8080"
echo "  curl http://localhost:8080/health"
echo ""

if [ "$DEPLOY_DASHBOARD" = true ]; then
    echo "Kubernetes Dashboard:"
    echo "  kubectl proxy"
    echo "  Visit: http://localhost:8001/api/v1/namespaces/kubernetes-dashboard/services/https:kubernetes-dashboard:/proxy/"
    echo "  Login token: kubectl -n kubernetes-dashboard create token admin-user"
    echo ""
    echo "Or use the access script:"
    echo "  ./deploy/k8s/access-dashboard.sh"
    echo ""
fi

echo "View Logs:"
echo "  kubectl logs -n zelana-prover -l app=coordinator"
echo "  kubectl logs -n zelana-prover -l app=prover-node"
echo ""

echo "Delete Deployment:"
echo "  kubectl delete -k $DEPLOY_DIR"
if [ "$DEPLOY_DASHBOARD" = true ]; then
    echo "  kubectl delete -f https://raw.githubusercontent.com/kubernetes/dashboard/v2.7.0/aio/deploy/recommended.yaml"
fi
echo ""
