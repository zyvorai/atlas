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
# protoc for the gRPC crates; cmake for the vendored librdkafka (kafka-lag feature).
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
COPY --from=ui /ui/dist crates/atlas-gateway/ui/dist
# Build the real MongoDB (pure Rust) + SQL Server (tiberius) + Oracle connectors and precise CDC lag in.
# The `oracle` crate vendors ODPI-C (compiles with the toolchain here) and dlopens the Oracle Instant
# Client at *runtime* — so no OCI libs are needed at build time, only in the runtime stage below.
RUN cargo build --release -p atlas-gateway -p atlas-cli \
    --features atlas-databridge/mongodb,atlas-databridge/sqlserver,atlas-databridge/oracle,atlas-databridge/kafka-lag

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
# ceph/rbd CLIs are only needed when ATLAS_CEPH_DRIVER_MODE=real against a real cluster.
# Oracle Instant Client (Basic Lite) + libaio provide libclntsh.so, which ODPI-C dlopens at runtime
# for the DataBridge `oracle` connector; freely redistributable. The URL is a build ARG so air-gapped
# builds can point at an internal mirror, e.g. --build-arg ORACLE_IC_URL=https://mirror.corp/…zip
ARG ORACLE_IC_URL=https://download.oracle.com/otn_software/linux/instantclient/2113000/instantclient-basiclite-linux.x64-21.13.0.0.0dbru.zip
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl unzip libaio1 \
    && curl -fsSL -o /tmp/ic.zip "$ORACLE_IC_URL" \
    && mkdir -p /opt/oracle && unzip -q /tmp/ic.zip -d /opt/oracle && rm /tmp/ic.zip \
    && apt-get purge -y --auto-remove curl unzip \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /var/lib/atlas atlas \
    && mkdir -p /var/lib/atlas && chown atlas:atlas /var/lib/atlas
ENV LD_LIBRARY_PATH=/opt/oracle/instantclient_21_13
COPY --from=builder /build/target/release/atlas-gateway /usr/local/bin/atlas-gateway
COPY --from=builder /build/target/release/atlasctl /usr/local/bin/atlasctl
COPY --from=builder /build/migrations /usr/local/share/atlas/migrations
USER atlas
WORKDIR /var/lib/atlas
ENV ATLAS_BIND_ADDR=0.0.0.0:5110 \
    ATLAS_DATABASE_URL=sqlite:///var/lib/atlas/atlas.db?mode=rwc
EXPOSE 5110
ENTRYPOINT ["/usr/local/bin/atlas-gateway"]
