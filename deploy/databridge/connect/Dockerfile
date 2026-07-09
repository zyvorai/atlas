# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# Kafka Connect image for DataBridge CDC: Strimzi Kafka base + the Debezium PostgreSQL source
# connector + the Aiven JDBC sink connector (+ the Postgres JDBC driver). Point the gateway at this
# image with ATLAS_DATABRIDGE_CONNECT_IMAGE so start_cdc's KafkaConnect can instantiate connectors.
#
# Build + import into k3s (on a cluster node):
#   podman build -t databridge-connect:dev -f deploy/databridge/connect/Dockerfile deploy/databridge/connect
#   podman save databridge-connect:dev -o /tmp/c.tar && sudo k3s ctr images import /tmp/c.tar
# Base must match the Strimzi operator's Kafka version (4.x: log4j2 layout). Debezium 3.x targets Kafka 4.
FROM quay.io/strimzi/kafka:latest-kafka-4.3.0
USER root

ARG DEBEZIUM_VERSION=3.0.8.Final
ARG AIVEN_JDBC_VERSION=6.10.0
ARG PG_JDBC_VERSION=42.7.3

RUN mkdir -p /opt/kafka/plugins/debezium-postgres /opt/kafka/plugins/jdbc-sink

# Debezium PostgreSQL source connector.
RUN curl -fsSL "https://repo1.maven.org/maven2/io/debezium/debezium-connector-postgres/${DEBEZIUM_VERSION}/debezium-connector-postgres-${DEBEZIUM_VERSION}-plugin.tar.gz" \
    | tar -xz -C /opt/kafka/plugins/debezium-postgres --strip-components=1

# Aiven JDBC connector (provides io.aiven.connect.jdbc.JdbcSinkConnector) + Postgres JDBC driver.
RUN curl -fsSL -o /tmp/jdbc.tar "https://github.com/Aiven-Open/jdbc-connector-for-apache-kafka/releases/download/v${AIVEN_JDBC_VERSION}/jdbc-connector-for-apache-kafka-${AIVEN_JDBC_VERSION}.tar" \
    && tar -xf /tmp/jdbc.tar -C /opt/kafka/plugins/jdbc-sink --strip-components=1 \
    && rm -f /tmp/jdbc.tar \
    && curl -fsSL -o /opt/kafka/plugins/jdbc-sink/postgresql.jar "https://jdbc.postgresql.org/download/postgresql-${PG_JDBC_VERSION}.jar"

USER 1001
