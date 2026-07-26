# Getting Started with Atlas

## What you need

| Requirement | Notes |
|-------------|--------|
| Atlas gateway | Default bind **`127.0.0.1:5110`** (REST + Storage Center UI) |
| Optional gRPC | **`:5111`** |
| Backend | Ceph via Rook/driver, or lab fake driver |
| Auth | Local often open (`ATLAS_AUTH_REQUIRED=0`); cluster uses JWT |

## 1. Open Storage Center

- Local: `http://127.0.0.1:5110/`
- Cluster NodePort (common): `http://<host>:30511/` (HTTPS may be **30543**)
- Health: `curl http://127.0.0.1:5110/health`

## 2. Sign in

| Mode | What you do |
|------|-------------|
| Open (lab) | No token when `ATLAS_AUTH_REQUIRED=0` |
| JWT | Paste bearer token from `POST /api/atlas/v1/auth/tokens` or secret |
| CLI | `atlasctl --token …` or `ATLAS_TOKEN` |

## 3. Orient yourself

1. **Command Deck** (`/`) — capacity and job health.
2. **Volumes / RBD / Snapshots** — block inventory.
3. **Jobs** — every mutation is a durable job (`202` + id).
4. **DataBridge** — database / object mobility.
5. Spotlight / dock for quick jumps.

## 4. First workflows

### A. Discover inventory

```bash
atlasctl health
atlasctl discover
atlasctl pools
atlasctl volumes
```

### B. Provision from intent

**Volumes** → create with a policy (`production`, `database`, …) → watch **Jobs**.

### C. Protect a volume

**Snapshots** / **Schedules** / **Backups** — confirm job completion.

### D. Check backend health

**Backends** → **Ceph** for day-2 OSD/pool/RGW signals.

## Next steps

- [Using the Dashboard](using-the-dashboard.md)
- [Admin basics](admin-basics.md)
- [Page guides](pages/README.md)
