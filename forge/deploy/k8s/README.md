# Zelana Prover – Kubernetes Deployment

A local guide for deploying the **Zelana Prover distributed system** on Kubernetes using **Minikube**.

---

## Prerequisites

Make sure you have the following installed:

* [Docker](https://www.docker.com/get-started)
* [Minikube](https://minikube.sigs.k8s.io/docs/start/)
* `kubectl` (CLI for Kubernetes)

Check versions:

```bash
docker --version
minikube version
kubectl version --client
```

---

## 1. Start Minikube Cluster

Start a local Kubernetes cluster:

```bash
minikube start --cpus=2 --memory=8g
```

Check status:

```bash
minikube status
```

* **Running** → cluster is active
* **Stopped** → cluster is off

---

## 2. Build Docker Images

From the project root:

```bash
./scripts/deploy-k8s.sh
```

**Notes:**

* This builds two images: `zelana/prover-node` and `zelana/prover-coordinator`.
* If using Minikube, images are automatically loaded into the cluster.

---

## 3. Deploy to Kubernetes

Deploy Zelana Forge to your Kubernetes cluster:

```bash
# Deploy Zelana Forge only
./scripts/deploy-k8s.sh

# Deploy Zelana Forge + Kubernetes Dashboard
./scripts/deploy-k8s.sh --with-dashboard
```

The deployment script:
* Builds Docker images for coordinator and prover nodes
* Loads images into Minikube (if using Minikube)
* Applies Kubernetes manifests from `deploy/k8s/`
* Creates namespace: `zelana-prover`
* Deploys StatefulSet: `prover-node` (5 replicas)
* Deploys Deployment: `coordinator` (1 replica)
* Creates Services: `coordinator` and `prover-node`
* Optionally deploys the official Kubernetes Dashboard

Check deployment status:

```bash
kubectl get pods -n zelana-prover
kubectl get services -n zelana-prover
```

If dashboard was deployed:

```bash
kubectl get pods -n kubernetes-dashboard
kubectl get services -n kubernetes-dashboard
```

You should see:
- `coordinator-*` pod (1 replica)
- `prover-node-*` pods (5 total, numbered 0-4)

---

## 4. Access Services

### Kubernetes Dashboard (Optional)

Deploy the official Kubernetes web UI alongside Zelana Forge:

#### Deploy with Dashboard
```bash
# Deploy both Zelana Forge and Dashboard
./scripts/deploy-k8s.sh --with-dashboard
```

#### Access Dashboard
```bash
# Use the access script
./deploy/k8s/access-dashboard.sh

# Or manually:
kubectl proxy
# Visit: http://localhost:8001/api/v1/namespaces/kubernetes-dashboard/services/https:kubernetes-dashboard:/proxy/
# Login token: kubectl -n kubernetes-dashboard create token admin-user
```

#### Dashboard Features
- **Workloads Overview**: View all pods, deployments, and stateful sets
- **Resource Monitoring**: CPU, memory, and storage usage graphs
- **Service Discovery**: View services, endpoints, and ingresses
- **Configuration**: Browse ConfigMaps and Secrets
- **Logs & Shell Access**: View pod logs and execute commands in pods
- **Namespace Filtering**: Focus on specific namespaces like `zelana-prover`
- **Scaling & Management**: Scale deployments and manage resources
- **Cluster Events**: View system events and troubleshooting info

#### Navigating Zelana Forge
1. **Login** with the admin token
2. **Select Namespace**: Choose `zelana-prover` from the namespace dropdown
3. **Workloads → Pods**: See all running pods:
   - `coordinator-*` (API server)
   - `prover-node-0/1/2/3/4` (ZK computation nodes)
4. **Services**: View `coordinator` and `prover-node` services
5. **Monitor Resources**: CPU/memory usage graphs for all components
6. **Access Logs**: Click any pod → "Logs" tab for real-time logs
7. **Exec Shell**: Click pod → "Exec" to run commands inside containers

**Dashboard Features:**
- Real-time cluster status and health
- Architecture visualization with ASCII diagrams
- Performance metrics and benchmarks
- Management commands reference
- Security features overview
- Auto-refresh every 30 seconds

### Coordinator

Forward the coordinator service port to your local machine:

```bash
kubectl port-forward -n zelana-prover svc/coordinator 8080:8080
```

Test the health endpoint:

```bash
curl http://localhost:8080/health
```

---

## 6. Monitoring & Observability

### Resource Usage

```bash
# View resource consumption
kubectl top pods -n zelana-prover

# View detailed pod metrics
kubectl top nodes
```

### Pod Health Checks

```bash
# Check pod readiness
kubectl get pods -n zelana-prover -o wide

# View pod events
kubectl get events -n zelana-prover --sort-by=.metadata.creationTimestamp

# Check pod resource limits
kubectl describe pods -n zelana-prover
```

### Network Monitoring

```bash
# View service endpoints
kubectl get endpoints -n zelana-prover

# Check network policies (if any)
kubectl get networkpolicies -n zelana-prover
```

---

## 7. View Logs

Coordinator logs:

```bash
kubectl logs -n zelana-prover -l app=coordinator
```

Prover node logs:

```bash
kubectl logs -n zelana-prover -l app=prover-node
```

View logs for specific pods:

```bash
kubectl logs -n zelana-prover coordinator-<pod-id>
kubectl logs -n zelana-prover prover-node-0
```

---

## 8. Stop or Delete Cluster

* **Stop Minikube (temporary, keeps data)**

```bash
minikube stop
```

* **Delete Minikube (removes cluster completely)**

```bash
minikube delete
```

---

## 9. Re-deploy

If the cluster was stopped or deleted:

```bash
minikube start
./scripts/deploy-k8s.sh                    # Redeploy Zelana Forge
./scripts/deploy-k8s.sh --with-dashboard   # Redeploy with Dashboard
```

---

## 10. Troubleshooting

* If `kubectl` cannot connect:

```bash
minikube start
kubectl config use-context minikube
```

* Ensure Docker is running locally and Minikube can access it:

```bash
docker ps
minikube docker-env
```

* If the Kubernetes Dashboard is not accessible:

```bash
# Check if dashboard pod is running
kubectl get pods -n zelana-prover -l app.kubernetes.io/name=kubernetes-dashboard

# Check dashboard logs
kubectl logs -n zelana-prover -l app.kubernetes.io/name=kubernetes-dashboard

# Restart dashboard if needed
kubectl rollout restart deployment/kubernetes-dashboard -n zelana-prover
```

* Dashboard shows "Unauthorized" errors:

The dashboard is configured with cluster-admin permissions for local development. If you see authorization issues, ensure you're accessing it through the port-forward and accepting the self-signed certificate.
