# Atlas — Customer Documentation

**Atlas** is the Zyvor **storage control plane** — products request storage intent; Atlas maps intent to backends (Ceph first), owns inventory/ownership/audit, and exposes Storage Center plus `atlasctl` / REST / gRPC.

| You want to… | Open |
|--------------|------|
| Install and log in | [Getting Started](getting-started.md) |
| Learn the shell | [Using the Dashboard](using-the-dashboard.md) |
| Follow a page, step by step | [Page-by-page guides](pages/README.md) |
| Look up any screen by route | [Complete page index](PAGE_INDEX.md) |
| Deploy, auth, ports | [Admin basics](admin-basics.md) |
| Multi-page jobs | [Common workflows](workflows.md) |
| Capability map | [Feature Guide](../atlas-customer-feature-guide.md) |

## Printable PDFs

```bash
node scripts/customer-docs/build-customer-pdfs.mjs
```

Output lands in [`pdf/`](pdf/):

| PDF | Contents |
|-----|----------|
| `Atlas-Customer-README.pdf` | This overview |
| `Atlas-Getting-Started.pdf` | Access, login, dashboard basics, workflows |
| `Atlas-Page-by-Page.pdf` | Complete page manual |
| `Atlas-Admin-Basics.pdf` | Deploy, auth, ports |

## Product at a glance

```text
  Storage Center  →  :5110  (UI + REST)
  gRPC            →  :5111
  CLI             →  atlasctl
  Drivers         →  Ceph (RBD / CephFS / RGW) first
```

## Support surfaces (quick map)

| Need | Typical path |
|------|----------------|
| Command Deck | `/` |
| Volumes | `/volumes` |
| Jobs | `/jobs` |
| Backups / buckets | `/backups`, `/buckets` |
| DataBridge | `/databridge/plans` |
| Backends / Ceph | `/backends`, `/ceph` |
| Access / tenants | `/access`, `/tenants` |

---

*ZyvorAI Labs · [zyvor.dev](https://zyvor.dev) · Atlas*
