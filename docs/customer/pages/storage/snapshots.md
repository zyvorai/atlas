# Snapshots

## Purpose

Volume and image snapshots for point-in-time recovery.

## When to use it

- Open this page when the job matches the purpose above
- Prefer **Command Deck** (`/`) first if you are unsure where to start
- Confirm gateway auth and backend connectivity if inventories look empty

## How to get there

- Route: `/snapshots`
- Nav: **STORAGE → Snapshots** (sidebar, dock, or spotlight)

## What you can do

1. Open `/snapshots` and wait for live data from the Atlas gateway (default **:5110**).
2. Use filters (backend, tenant, kind, status) when the page provides them.
3. Drill into a volume, job, or plan for detail — mutations return durable jobs (`202` + job id).
4. For mutating actions (provision, backup, migrate, DR): review tenant quotas and job status in **Jobs**.

If the page stays empty, check `/health`, auth (`ATLAS_AUTH_REQUIRED` / JWT), that a storage driver is registered, and run `atlasctl discover` if inventory is cold.

## Related pages

- [Getting Started](../../getting-started.md)
- [Command Deck](../storage/home.md)
- [Volumes](../storage/volumes.md)
- [Page index](../../PAGE_INDEX.md)
