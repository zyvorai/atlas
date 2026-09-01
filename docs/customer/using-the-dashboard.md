# Using the Dashboard

Storage Center is the Atlas operator console on the gateway (`http://<host>:5110/`).

## Shell chrome

Navigation lives in a persistent **left sidebar**, grouped by section; collapse it to an
icon-only rail from the toggle at its base (state remembered per browser). The top bar carries
everything else:

| Control | What it does |
|---------|----------------|
| Sidebar | STORAGE · DATA PROTECTION · DATABRIDGE · OBSERVABILITY · GOVERNANCE · INFRASTRUCTURE — click the brand mark to jump to **Command Deck** |
| Spotlight | **⌘K** / **Ctrl+K** — pools, volumes, modules |
| Look & feel | Carbon (dark shop, default) or Apple Lite (light shop) |
| Jobs spinner | Shows while jobs are running → `/jobs` |
| Alerts bell | Open alert count → `/alerts` |
| Pause | Freeze auto-refresh |
| Health pill | Ceph/Atlas rollup (HEALTH_OK, DEGRADED, …) |
| Account menu | Clock, paste/clear JWT, sign out |

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
