<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# Atlas

[![CI](https://github.com/zyvorai/atlas/actions/workflows/ci.yml/badge.svg)](https://github.com/zyvorai/atlas/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.3.0-informational)](CHANGELOG.md)
[![Docs](https://img.shields.io/badge/docs-zyvorai.github.io%2Fatlas-blue)](https://zyvorai.github.io/atlas/)

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React-20232A?logo=react&logoColor=61DAFB)
![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-07405E?logo=sqlite&logoColor=white)
![gRPC](https://img.shields.io/badge/gRPC-4285F4?logo=grpc&logoColor=white)
![Ceph](https://img.shields.io/badge/Ceph-EF5C55?logo=ceph&logoColor=white)

![Atlas — Storage, as a product.](docs/social/atlas-share-card.png)

**Storage, as a product.** Atlas is the **central storage control plane** for the Zyvor
suite. Products call stable Atlas APIs; Atlas maps intent to Ceph (and NFS/ZFS) through
pluggable drivers — with an Apple Shop console for operators.

**3** storage backends · **6** database engines migratable via DataBridge · **80+** REST
endpoints · **3** access surfaces (REST · gRPC · SSE)

📖 **[Read the full docs](https://zyvorai.github.io/atlas/)** — quickstart, architecture, licensing.

![Atlas Storage Center — Overview](docs/ux/00-overview.png)

## Contents

- [Quickstart](#-quickstart)
- [Dashboard gallery](#-dashboard-gallery)
- [Architecture at a glance](#-architecture-at-a-glance)
- [Capabilities](#-capabilities)
- [Why Atlas](#-why-atlas)
- [Important boundaries](#-important-boundaries)
- [License](#-license)

## 🚀 Quickstart

```bash
make run
# Console → http://127.0.0.1:5110
cargo run -p atlas-cli -- --base-url http://127.0.0.1:5110 health
```

No Ceph cluster needed to try it — `make run` starts the gateway against a fake driver so
you can click through the whole console immediately.

Deploy to a remote k3s host:

```bash
./scripts/deploy-remote.sh <host> <user>
# UI → http://<host>:30510
```

| Track | Where |
| --- | --- |
| **Self-host from source** (AGPL, free for home) | This repo |
| **Commercial license (ACL)** | [sales@zyvor.dev](mailto:sales@zyvor.dev) · [COMMERCIAL_LICENSE.md](COMMERCIAL_LICENSE.md) |
| **Docs site** | https://zyvorai.github.io/atlas/ |

More: [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md) · [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) · [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🖥 Dashboard gallery

Live console shots (captured against a lab deployment):

| | | |
|---|---|---|
| ![Overview](docs/ux/00-overview.png) | ![Volumes](docs/ux/01-volumes.png) | ![Observatory](docs/ux/02-observatory.png) |
| ![Ceph](docs/ux/03-ceph.png) | ![DataBridge](docs/ux/04-databridge.png) | ![Sign in](docs/ux/05-login.png) |

Full tour: [Gallery](https://zyvorai.github.io/atlas/gallery).

## 🗺 Architecture at a glance

```mermaid
flowchart LR
  subgraph Products["Zyvor products"]
    P1["Zeus OS / v9s"]
    P2["Veyron"]
    P3["HyperSDK · Aether · …"]
  end
  Products -- "REST · gRPC" --> Atlas["Atlas Gateway"]
  Atlas --> Driver["StorageDriver trait"]
  Driver --> Ceph[("Ceph\nRBD · CephFS · RGW")]
  Driver --> NFS[("NFS")]
  Driver --> ZFS[("ZFS")]
  Atlas --> DataBridge["DataBridge"]
  DataBridge --> Edge[("Edge DB on Ceph\nPostgres · MySQL · MariaDB\nOracle · SQL Server · MongoDB")]
```

Full write-up: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🧰 Capabilities

- **Intent → storage** — volumes, snapshots, clones, CephFS RWX, RGW buckets via REST + gRPC
- **Pluggable drivers** — real Ceph first; NFS + ZFS; fake driver for local demo
- **DataBridge** — cloud-to-edge DB migration (six engines, CDC, cutover) on Ceph
- **Day-2** — alerts, maintenance, governance, quotas, upgrade preflight, DR scaffolding
- **Ops Advisor** — explainable AI-assisted risk scoring and prioritized, read-only runbooks
- **Console** — Apple.com-style top-nav shell, SF type, Night/Day themes

Customer-facing feature guide: [docs/atlas-customer-feature-guide.md](docs/atlas-customer-feature-guide.md).

## ⚖ Why Atlas

| | Atlas | Raw Ceph tooling | Rook alone |
|---|---|---|---|
| Intent-based API (not pool internals) | Yes | No | No |
| One API across Ceph + NFS + ZFS | Yes | Ceph only | Ceph only |
| Cloud-to-edge DB migration (DataBridge) | Yes | No | No |
| Per-tenant quotas & governance | Yes | Partial | No |
| Built-in operator console | Yes | Ceph Dashboard only | No |
| Kubernetes-native provisioning | Yes (via drivers) | No | Yes |

## 🔍 Important boundaries

What's free under AGPL vs. what needs a commercial license
([full guide](docs/LICENSING.md)):

| Use case | Allowed under AGPL? |
| --- | --- |
| Self-host for home or your own operations | Yes, free |
| Modify for internal use | Yes, free |
| Build and publish your own AGPL extensions | Yes, free |
| Deploy modified Atlas as public SaaS without releasing changes | No — needs ACL |
| Embed Atlas in a closed-source product | No — needs ACL |
| White-label proprietary customizations without AGPL | No — needs ACL |

## 📈 Star History

[![Star History Chart](https://api.star-history.com/svg?repos=zyvorai/atlas&type=Date)](https://star-history.com/#zyvorai/atlas&Date)

## 📄 License

Dual-licensed:

- **[AGPL-3.0](LICENSE)** — open source; free for home users and self-host under AGPL terms
- **[Atlas Commercial License (ACL)](COMMERCIAL_LICENSE.md)** — proprietary integrations, freedom from AGPL obligations, support

See [docs/LICENSING.md](docs/LICENSING.md). Contributions: [CLA.md](CLA.md) + [DCO.md](DCO.md) (`git commit -s`),
governed by our [Code of Conduct](CODE_OF_CONDUCT.md).
