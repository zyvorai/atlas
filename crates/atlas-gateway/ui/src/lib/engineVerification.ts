// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Honest live-verification matrix for DataBridge engines (mirrors docs/DATABRIDGE.md). */

export type EngineLiveStage =
  | "discover"
  | "full-load"
  | "validate"
  | "cdc"
  | "cutover";

export type EngineVerification = {
  engine: string;
  /** Stages verified live on real infra (not fake). */
  live: EngineLiveStage[];
  note: string;
};

export const ENGINE_VERIFICATION: EngineVerification[] = [
  {
    engine: "postgres",
    live: ["discover", "full-load", "validate", "cdc", "cutover"],
    note: "End-to-end incl. Debezium CDC on Rook Ceph.",
  },
  {
    engine: "mysql",
    live: ["discover", "full-load", "validate"],
    note: "CDC + cutover need Kafka/Debezium on the edge cluster.",
  },
  {
    engine: "mariadb",
    live: ["discover", "full-load", "validate"],
    note: "CDC + cutover need Kafka/Debezium on the edge cluster.",
  },
  {
    engine: "mongodb",
    live: ["discover", "full-load", "validate"],
    note: "CDC + cutover need Kafka/Debezium + Mongo sink image.",
  },
  {
    engine: "sqlserver",
    live: ["discover"],
    note: "Heterogeneous → Postgres; full-load is Debezium initial snapshot (CDC path).",
  },
  {
    engine: "oracle",
    live: ["discover"],
    note: "Heterogeneous → Postgres; full-load is Debezium initial snapshot (CDC path).",
  },
];

export function verificationFor(kind: string): EngineVerification | undefined {
  const k = kind.toLowerCase();
  return ENGINE_VERIFICATION.find((e) => e.engine === k);
}

export function stageBadge(
  v: EngineVerification | undefined,
  stage: EngineLiveStage,
): "live" | "pending" {
  return v?.live.includes(stage) ? "live" : "pending";
}
