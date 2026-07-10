# Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
# Kafka Connect image for DataBridge CDC: Strimzi Kafka base + Debezium source connectors
# (PostgreSQL, MySQL/MariaDB, MongoDB) + the Aiven JDBC sink connector (with Postgres + MySQL JDBC
# drivers) + the MongoDB Kafka sink. One image serves every DataBridge engine. Point the gateway at
# it with ATLAS_DATABRIDGE_CONNECT_IMAGE so start_cdc's KafkaConnect can instantiate the connectors.
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
ARG MYSQL_JDBC_VERSION=8.4.0
ARG MONGO_SINK_VERSION=1.15.0

RUN mkdir -p /opt/kafka/plugins/debezium-postgres /opt/kafka/plugins/debezium-mysql \
             /opt/kafka/plugins/debezium-mongodb /opt/kafka/plugins/jdbc-sink /opt/kafka/plugins/mongo-sink

# Debezium source connectors: PostgreSQL, MySQL/MariaDB, MongoDB.
RUN curl -fsSL "https://repo1.maven.org/maven2/io/debezium/debezium-connector-postgres/${DEBEZIUM_VERSION}/debezium-connector-postgres-${DEBEZIUM_VERSION}-plugin.tar.gz" \
    | tar -xz -C /opt/kafka/plugins/debezium-postgres --strip-components=1
RUN curl -fsSL "https://repo1.maven.org/maven2/io/debezium/debezium-connector-mysql/${DEBEZIUM_VERSION}/debezium-connector-mysql-${DEBEZIUM_VERSION}-plugin.tar.gz" \
    | tar -xz -C /opt/kafka/plugins/debezium-mysql --strip-components=1
RUN curl -fsSL "https://repo1.maven.org/maven2/io/debezium/debezium-connector-mongodb/${DEBEZIUM_VERSION}/debezium-connector-mongodb-${DEBEZIUM_VERSION}-plugin.tar.gz" \
    | tar -xz -C /opt/kafka/plugins/debezium-mongodb --strip-components=1

# Aiven JDBC sink connector (io.aiven.connect.jdbc.JdbcSinkConnector) + Postgres & MySQL JDBC drivers.
RUN curl -fsSL -o /tmp/jdbc.tar "https://github.com/Aiven-Open/jdbc-connector-for-apache-kafka/releases/download/v${AIVEN_JDBC_VERSION}/jdbc-connector-for-apache-kafka-${AIVEN_JDBC_VERSION}.tar" \
    && tar -xf /tmp/jdbc.tar -C /opt/kafka/plugins/jdbc-sink --strip-components=1 \
    && rm -f /tmp/jdbc.tar \
    && curl -fsSL -o /opt/kafka/plugins/jdbc-sink/postgresql.jar "https://jdbc.postgresql.org/download/postgresql-${PG_JDBC_VERSION}.jar" \
    && curl -fsSL -o /opt/kafka/plugins/jdbc-sink/mysql-connector-j.jar "https://repo1.maven.org/maven2/com/mysql/mysql-connector-j/${MYSQL_JDBC_VERSION}/mysql-connector-j-${MYSQL_JDBC_VERSION}.jar"

# MongoDB Kafka sink connector (com.mongodb.kafka.connect.MongoSinkConnector), fat "-all" jar.
RUN curl -fsSL -o /opt/kafka/plugins/mongo-sink/mongo-kafka-connect-all.jar \
    "https://repo1.maven.org/maven2/org/mongodb/kafka/mongo-kafka-connect/${MONGO_SINK_VERSION}/mongo-kafka-connect-${MONGO_SINK_VERSION}-all.jar"

USER 1001
