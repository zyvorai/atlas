# Admin Basics (Atlas)

## Ports

| Port | Service |
|------|---------|
| **5110** | REST + Storage Center UI (`ATLAS_BIND_ADDR`) |
| **5111** | gRPC (`ATLAS_GRPC_ADDR`; empty disables) |
| **30511** | Common cluster REST/UI NodePort |
| **30512** | Common gRPC NodePort |
| **30543** | Optional HTTPS NodePort |

## Auth

- Lab: `ATLAS_AUTH_REQUIRED=0` (open).
- Production: `ATLAS_AUTH_REQUIRED=1` + JWT secret (`atlas-gateway-auth`); weak defaults rejected at start.
- Mint tokens: `POST /api/atlas/v1/auth/tokens`.
- CLI: `--token` / `ATLAS_TOKEN`; API base `/api/atlas/v1/...`.
- Unauthenticated: `/health`, `/metrics`.

## Install sketch

```bash
make run          # local gateway + UI
atlasctl health
atlasctl discover
```

Cluster: follow `docs/DEPLOYMENT.md` / Ceph gateway manifests. Mutations return `202` + job id — watch via UI **Jobs** or gRPC `WatchJob`.

## Related

- [Getting Started](getting-started.md)
