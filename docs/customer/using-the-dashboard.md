# Using the Dashboard

Atlas **Storage Center** is a React SPA served by the gateway, organized by six nav sections.

## Surfaces

| Element | Purpose |
|---------|---------|
| **Command Deck** | Home — capacity, jobs, shortcuts |
| **STORAGE / DATA PROTECTION / DATABRIDGE / OBSERVABILITY / GOVERNANCE / INFRASTRUCTURE** | Sidebar sections |
| **Dock** | Pinned modules (volumes, jobs, backups, DataBridge, …) |
| **Spotlight** | Jump to any module by label |
| **Login gate** | Bearer token entry when auth is required |

## Browse vs act

Inventories and metrics are safe to explore. Provisions, deletes, migrations, and DR actions enqueue durable jobs — always confirm in **Jobs** before assuming success.

## Related

- [Getting Started](getting-started.md)
- [Page-by-page guides](pages/README.md)
