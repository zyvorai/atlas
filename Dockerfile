# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# ---- UI builder (React Storage Center → dist) ----
FROM node:22-alpine AS ui
WORKDIR /ui
COPY crates/atlas-gateway/ui/package.json crates/atlas-gateway/ui/package-lock.json ./
RUN npm ci
COPY crates/atlas-gateway/ui/ ./
RUN npm run build

# ---- builder ----
FROM rust:1.88-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
COPY --from=ui /ui/dist crates/atlas-gateway/ui/dist
RUN cargo build --release -p atlas-gateway -p atlas-cli

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
# ceph/rbd CLIs are only needed when ATLAS_CEPH_DRIVER_MODE=real against a real cluster.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /var/lib/atlas atlas \
    && mkdir -p /var/lib/atlas && chown atlas:atlas /var/lib/atlas
COPY --from=builder /build/target/release/atlas-gateway /usr/local/bin/atlas-gateway
COPY --from=builder /build/target/release/atlasctl /usr/local/bin/atlasctl
COPY --from=builder /build/migrations /usr/local/share/atlas/migrations
USER atlas
WORKDIR /var/lib/atlas
ENV ATLAS_BIND_ADDR=0.0.0.0:5110 \
    ATLAS_DATABASE_URL=sqlite:///var/lib/atlas/atlas.db?mode=rwc
EXPOSE 5110
ENTRYPOINT ["/usr/local/bin/atlas-gateway"]
