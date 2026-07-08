#!/usr/bin/env bash
# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# Stand up a self-contained Prometheus + Grafana that scrape/visualize the Atlas control plane.
# Requires kubectl pointed at the cluster running atlas-gateway-ceph (namespace rook-ceph).
set -euo pipefail
cd "$(dirname "$0")"
NS=rook-ceph

echo "==> Prometheus (scrapes atlas /metrics + ceph mgr)"
kubectl apply -f prometheus.yaml

echo "==> Grafana dashboard ConfigMap (from atlas-overview.json)"
kubectl -n "$NS" create configmap atlas-grafana-dashboard \
  --from-file=atlas-overview.json=atlas-overview.json \
  --dry-run=client -o yaml | kubectl apply -f -

echo "==> Grafana (datasource + dashboard auto-provisioned)"
kubectl apply -f grafana.yaml

echo "==> waiting for rollouts"
kubectl -n "$NS" rollout status deploy/atlas-prometheus --timeout=180s
kubectl -n "$NS" rollout status deploy/atlas-grafana --timeout=180s

NODE_IP="$(kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type=="InternalIP")].address}')"
cat <<EOF

Observability is up:
  Prometheus  http://${NODE_IP}:30514   (targets: Status -> Targets; atlas-gateway should be UP)
  Grafana     http://${NODE_IP}:30515   (admin / atlas -> Dashboards -> Atlas -> "Atlas Storage Control Plane")
EOF
