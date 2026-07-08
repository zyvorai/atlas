// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// TypeScript mirrors of the Atlas DTOs (crates/atlas-api-types/src/lib.rs).

export type Health = "ok" | "warn" | "critical" | "unknown";

export interface StorageVolume {
  id: string;
  cluster_id?: string | null;
  pool_id?: string | null;
  name: string;
  kind: "block" | "filesystem" | "object";
  backend_native_id?: string | null;
  size_bytes: number;
  used_bytes?: number | null;
  state: string;
  health: Health;
  kubernetes_namespace?: string | null;
  pvc_name?: string | null;
  storage_class_name?: string | null;
}

export interface StorageSnapshot {
  id: string;
  tenant_id: string;
  volume_id: string;
  name: string;
  backend_native_id?: string | null;
  consistency: string;
  state: string;
  protected: boolean;
  parent_snapshot_id?: string | null;
  created_at?: string | null;
}

export interface StorageBucket {
  id: string;
  tenant_id: string;
  name: string;
  bucket_name?: string | null;
  endpoint?: string | null;
  region?: string | null;
  secret_ref?: string | null;
  namespace?: string | null;
  state: string;
  created_at?: string | null;
}

export interface BackupRecord {
  id: string;
  tenant_id: string;
  volume_id: string;
  snapshot_id?: string | null;
  bucket_id: string;
  object_key: string;
  format: string;
  checksum?: string | null;
  state: string;
  created_at?: string | null;
}

export interface SnapshotSchedule {
  id: string;
  tenant_id: string;
  volume_id: string;
  kind: "snapshot" | "backup";
  bucket_id?: string | null;
  mode: string;
  interval_secs: number;
  keep: number;
  enabled: boolean;
  last_run_at?: string | null;
  next_run_at: string;
  created_at?: string | null;
}

export interface TenantQuota {
  tenant_id: string;
  max_bytes: number;
  max_volumes: number;
  used_bytes: number;
  volume_count: number;
}

export interface TenantPolicy {
  tenant_id: string;
  intent: string;
  storage_class: string;
  access_mode: string;
  volume_mode: string;
}

export interface AlertRecord {
  id: string;
  severity: string;
  source: string;
  resource_type: string;
  resource_id: string;
  title: string;
  description: string;
  evidence?: unknown;
  state: string;
  created_at?: string | null;
  resolved_at?: string | null;
}

export interface JobRecord {
  id: string;
  tenant_id: string;
  job_type: string;
  state: string;
  requested_by: string;
  progress_percent: number;
  error?: string | null;
  result?: unknown;
  created_at?: string | null;
  updated_at?: string | null;
}

export interface StorageCluster {
  id: string;
  backend_id: string;
  name: string;
  native_fsid?: string | null;
  health: Health;
  raw_capacity_bytes?: number | null;
  used_capacity_bytes?: number | null;
  available_capacity_bytes?: number | null;
}

export interface StoragePool {
  id: string;
  cluster_id: string;
  name: string;
  kind: string;
  device_class?: string | null;
  replica_size?: number | null;
  used_bytes?: number | null;
  max_bytes?: number | null;
  health: Health;
}

export interface Osd {
  id: number;
  cluster_id: string;
  osd_num?: number;
  up: boolean;
  in_cluster: boolean;
  device_class?: string | null;
  host?: string | null;
  used_bytes?: number | null;
  capacity_bytes?: number | null;
}

export interface MetricsSummary {
  raw_capacity_bytes: number;
  used_capacity_bytes: number;
  available_capacity_bytes: number;
  used_capacity_percent: number;
  clusters: number;
  pools: number;
  volumes: number;
  snapshots: number;
  buckets: number;
  backups: number;
  client_io: {
    read_ops_total: number;
    write_ops_total: number;
    read_bytes_total: number;
    write_bytes_total: number;
  };
  recovery: {
    pg_recovering: number;
    pg_backfilling: number;
    objects_degraded: number;
    objects_misplaced: number;
    objects_unfound: number;
  };
}

export interface MetricSample {
  name: string;
  value: number;
  labels: Record<string, string>;
}

// GET /metrics/forecast — least-squares fill projection; days_to_full is null when not growing.
export interface MetricForecast {
  samples: number;
  window_minutes: number;
  used_capacity_bytes: number;
  raw_capacity_bytes: number;
  growth_bytes_per_day: number;
  days_to_full: number | null;
}

// One persisted time-series row from GET /metrics/history (read_ops/write_ops are cumulative totals).
export interface MetricHistoryPoint {
  ts: string;
  raw_capacity_bytes: number;
  used_capacity_bytes: number;
  volumes: number;
  snapshots: number;
  read_bytes: number;
  write_bytes: number;
  read_ops: number;
  write_ops: number;
  jobs_running: number;
  alerts_open: number;
}

export interface AuditRow {
  id: number;
  tenant_id?: string | null;
  actor_id: string;
  action: string;
  resource_type: string;
  resource_id: string;
  status: string;
  request?: unknown;
  result?: unknown;
  created_at: string;
}

export interface JobAccepted {
  job_id?: string;
  state?: string;
  resource?: Record<string, unknown>;
  links?: { job?: string };
}
