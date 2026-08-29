# Using the Dashboard

Storage Center is the Atlas operator console on the gateway (`http://<host>:5110/`).

## Shell chrome

| Control | What it does |
|---------|----------------|
| Brand / wordmark | Jump to **Command Deck** |
| Section menus | STORAGE · DATA PROTECTION · DATABRIDGE · OBSERVABILITY · GOVERNANCE · INFRASTRUCTURE |
| Menubar chips | Deck, Volumes, Jobs, Alerts, Ceph, Observatory, Settings |
| Spotlight | **⌘K** / **Ctrl+K** — pools, volumes, modules |
| Jobs / Alerts | Live chips → `/jobs` and `/alerts` |
| Pause | Freeze auto-refresh |
| Health | Ceph/Atlas rollup (HEALTH_OK, DEGRADED, …) |
| Key / Sign out | Paste JWT or clear session |
| Look & feel | Carbon, Nebula, Dark steel, Zinc metal, Aurora |

## Page grammar

Most screens share: **PageHead** (eyebrow + title + live state + ≤2 actions) → table/canvas → SlideOver or FormModal for mutations.

Empty states name the next action (Create volume, Create bucket, Register source, …) — follow those CTAs rather than shrugging.

## Operate tips

1. After any **Create** / **Backup** / **Failover**, open **Jobs** until `succeeded`.
2. Prefer console actions for day-2; use `atlasctl` for scripts and CI.
3. DataBridge: register **Cloud Databases** → **Migration Plans** → Plan Detail (CDC / cutover) → **Validation**.
4. Never paste lab IPs into runbooks — use `<host>`.

## Related

- [Getting Started](getting-started.md)
- [Page index](PAGE_INDEX.md)
- [Workflows](workflows.md)
