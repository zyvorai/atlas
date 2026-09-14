<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. -->
<!-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial -->
# Atlas

[![CI](https://github.com/zyvorai/atlas/actions/workflows/ci.yml/badge.svg)](https://github.com/zyvorai/atlas/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.2.0-informational)](CHANGELOG.md)

![Atlas — Survey the cluster. Provision with intent. Operate day-2.](docs/social/atlas-share-card.png)

**Central storage control plane** for the Zyvor suite. Products call stable Atlas APIs;
Atlas maps intent to Ceph (and NFS/ZFS) through pluggable drivers — with an Apple Shop
console for operators.

📖 **[Read the full docs](https://zyvorai.github.io/atlas/)** — quickstart, architecture, licensing.

![Atlas Storage Center — Overview](docs/ux/00-overview.png)

## Contents

- [Dashboard gallery](#dashboard-gallery)
- [Capabilities](#capabilities)
- [Quickstart](#quickstart)
- [Important boundaries](#important-boundaries)
- [License](#license)

## Dashboard gallery

Live console shots (captured against a lab deployment):

| | | |
|---|---|---|
| ![Overview](docs/ux/00-overview.png) | ![Volumes](docs/ux/01-volumes.png) | ![Observatory](docs/ux/02-observatory.png) |
| ![Ceph](docs/ux/03-ceph.png) | ![DataBridge](docs/ux/04-databridge.png) | ![Sign in](docs/ux/05-login.png) |

Full tour: [Gallery](https://zyvorai.github.io/atlas/gallery).

## Capabilities

- **Intent → storage** — volumes, snapshots, clones, CephFS RWX, RGW buckets via REST + gRPC
- **Pluggable drivers** — real Ceph first; NFS + ZFS; fake driver for local demo
- **DataBridge** — cloud-to-edge DB migration (six engines, CDC, cutover) on Ceph
- **Day-2** — alerts, maintenance, governance, quotas, upgrade preflight, DR scaffolding
- **Console** — Carbon / Apple Lite shells, SF type, Apple Blue CTAs

Customer-facing feature guide: [docs/atlas-customer-feature-guide.md](docs/atlas-customer-feature-guide.md).

## Quickstart

```bash
make run
# Console → http://127.0.0.1:5110
cargo run -p atlas-cli -- --base-url http://127.0.0.1:5110 health
```

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

## Important boundaries

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

## License

Dual-licensed:

- **[AGPL-3.0](LICENSE)** — open source; free for home users and self-host under AGPL terms
- **[Atlas Commercial License (ACL)](COMMERCIAL_LICENSE.md)** — proprietary integrations, freedom from AGPL obligations, support

See [docs/LICENSING.md](docs/LICENSING.md). Contributions: [CLA.md](CLA.md) + [DCO.md](DCO.md) (`git commit -s`).
