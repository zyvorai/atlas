// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// react-query hooks for the Atlas API. Queries auto-refetch; write helpers live in views.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { http } from "./client";
import { useUi } from "../store/ui";
import type {
  ActivityEvent, AlertRecord, AuditRow, BackupRecord, JobRecord, MetricForecast, MetricHistoryPoint, MetricSample, MetricsSummary, Osd,
  SnapshotSchedule, StorageBucket, StorageCluster, StoragePool, StorageSnapshot, StorageVolume,
  TenantPolicy, TenantQuota,
  MigrationSource, MigrationPlan, EdgeDbCluster, CdcStream, ValidationRun,
} from "./types";

const g = async <T,>(path: string): Promise<T> => (await http.get<T>(path)).data;

function q<T>(key: unknown[], path: string, refetch = 8000, enabled = true) {
  // Pause auto-refresh globally when the user toggles it (menu bar).
  return useQuery<T>({
    queryKey: key,
    queryFn: () => g<T>(path),
    refetchInterval: () => (useUi.getState().paused ? false : refetch),
    enabled,
  });
}

export const useSummary = () => q<MetricsSummary>(["summary"], "/metrics/summary", 6000);
export const useHistory = () =>
  q<MetricHistoryPoint[]>(["history"], "/metrics/history?minutes=60", 30000);
export const useForecast = () =>
  q<MetricForecast>(["forecast"], "/metrics/forecast?minutes=1440", 30000);
export const useClusters = () => q<StorageCluster[]>(["clusters"], "/clusters", 10000);
export const usePools = () => q<StoragePool[]>(["pools"], "/pools", 10000);
export const useOsds = () => q<Osd[]>(["osds"], "/osds", 10000);
export const useNodes = () => q<{ host: string }[]>(["nodes"], "/nodes", 15000);
export const useVolumes = (state?: string, tenant?: string, backend?: string, kind?: string) => {
  const qs = new URLSearchParams();
  if (state) qs.set("state", state);
  if (tenant) qs.set("tenant", tenant);
  if (backend) qs.set("backend", backend);
  if (kind) qs.set("kind", kind);
  const s = qs.toString();
  return q<StorageVolume[]>(["volumes", state, tenant, backend, kind], `/volumes${s ? "?" + s : ""}`, 6000);
};
export const useSnapshots = () => q<StorageSnapshot[]>(["snapshots"], "/snapshots", 6000);
export const useSchedules = (volumeId?: string) =>
  q<SnapshotSchedule[]>(["schedules", volumeId], `/schedules${volumeId ? "?volume_id=" + volumeId : ""}`, 8000);
export const useBackups = (volumeId?: string) =>
  q<BackupRecord[]>(["backups", volumeId], `/backups${volumeId ? "?volume_id=" + volumeId : ""}`, 6000);
export const useBuckets = () => q<StorageBucket[]>(["buckets"], "/buckets", 8000);
export const useAlerts = (state?: string) =>
  q<AlertRecord[]>(["alerts", state], `/alerts${state ? "?state=" + state : ""}`, 6000);
export const useJobs = () => q<JobRecord[]>(["jobs"], "/jobs", 4000);
export const useAudit = (params: string) => q<AuditRow[]>(["audit", params], `/audit${params}`, 10000);
export const useEvents = (limit = 100) =>
  q<ActivityEvent[]>(["events", limit], `/events?limit=${limit}`, 8000);
export const useTenants = () => q<TenantQuota[]>(["tenants"], "/tenants", 10000);
export const useTenantPolicies = (id: string) =>
  q<TenantPolicy[]>(["tenantPolicies", id], `/tenants/${id}/policies`, 15000, !!id);
export const useCephMetrics = (prefix?: string) =>
  q<MetricSample[]>(["cephMetrics", prefix], `/metrics/ceph${prefix ? "?prefix=" + prefix : ""}`, 8000);
export const usePolicies = () => q<any[]>(["policies"], "/policies", 60000);
export const useBackends = () => q<any[]>(["backends"], "/backends", 20000);
export const useBackendsSummary = () => q<any[]>(["backends-summary"], "/backends/summary", 15000);

// Day-2 operations.
export const useMaintenance = () => q<{ paused: boolean }>(["maintenance"], "/maintenance", 8000);
export const usePreflight = () =>
  q<{ ready: boolean; checks: { check: string; ok: boolean; detail: string }[]; blockers: string[] }>(
    ["preflight"], "/upgrade/preflight", 10000);
export const useOrphans = () => q<{ orphan_backups: any[]; count: number }>(["orphans"], "/maintenance/orphans", 20000);
export const useDrStatus = () =>
  q<{
    peers: number;
    mirrors: number;
    primary: number;
    secondary: number;
    worst_rpo_seconds: number | null;
    control_plane_ready?: boolean;
    dataplane_verified?: boolean;
    verified?: boolean;
    note?: string;
  }>(["dr-status"], "/dr/status", 10000);
export const useDrPeers = () => q<any[]>(["dr-peers"], "/dr/peers", 15000);
export const useDrMirrors = () => q<any[]>(["dr-mirrors"], "/dr/mirrors", 10000);
export const useDrPreflight = () =>
  q<{
    ready: boolean;
    control_plane_ready?: boolean;
    dataplane_verified?: boolean;
    checks: { id: string; ok: boolean; detail: string }[];
    blockers: string[];
    warnings?: string[];
  }>(["dr-preflight"], "/dr/preflight", 10000);
export const useCephStatus = () => q<any>(["ceph-status"], "/ceph/status", 8000);
export const useCephOsdTree = () => q<any>(["ceph-osd-tree"], "/ceph/osd-tree", 15000);
export const useCephOsdDf = () => q<any>(["ceph-osd-df"], "/ceph/osd-df", 12000);
export const useCephDf = () => q<any>(["ceph-df"], "/ceph/df", 12000);
export const useStorageClasses = () => q<any[]>(["scs"], "/storage-classes", 20000);
export const useRbdImages = (pool: string) =>
  q<{ pool: string; images: string[] }>(["rbd", pool], `/rbd-images?pool=${pool}`, 8000);

// ---- DataBridge ----
export const useSources = () => q<MigrationSource[]>(["db-sources"], "/databridge/sources", 6000);
export const useSource = (id: string) => q<MigrationSource>(["db-source", id], `/databridge/sources/${id}`, 4000, !!id);
export const usePlans = () => q<MigrationPlan[]>(["db-plans"], "/databridge/plans", 6000);
export const usePlan = (id: string) => q<MigrationPlan>(["db-plan", id], `/databridge/plans/${id}`, 4000);
export const useEdgeClusters = () => q<EdgeDbCluster[]>(["db-edge"], "/databridge/edge-clusters", 8000);
export const useCdcStreams = () => q<CdcStream[]>(["db-cdc"], "/databridge/cdc-streams", 4000);
export const useValidations = (planId?: string) =>
  q<ValidationRun[]>(["db-val", planId], `/databridge/validations${planId ? "?plan_id=" + planId : ""}`, 6000);

export function useInvalidate() {
  const qc = useQueryClient();
  return (...keys: string[]) => keys.forEach((k) => qc.invalidateQueries({ queryKey: [k] }));
}
