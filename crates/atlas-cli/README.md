<!-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved. -->
# atlas-cli (`atlasctl`)

A thin REST client for the Atlas gateway.

```bash
atlasctl [--base-url URL] [--token JWT] <command>
```

Commands: `health`, `version`, `backends`, `discover [backend]`, `clusters`, `pools`, `osds`,
`volumes`, `storage-classes`, `metrics`. Output is pretty-printed JSON; non-2xx exits non-zero.

Env: `ATLAS_BASE_URL` (default `http://127.0.0.1:5110`), `ATLAS_TOKEN`.

```bash
atlasctl discover                       # POST /backends/bkd_ceph_lab/discover
atlasctl --base-url http://host:30511 pools
```
