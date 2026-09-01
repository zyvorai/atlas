// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Single source for sidebar nav, spotlight, and App route paths.
import type { LucideIcon } from "lucide-react";
import type { ComponentType } from "react";
import {
  Activity,
  Aperture,
  Archive,
  BadgeCheck,
  Bell,
  BookOpen,
  Boxes,
  Camera,
  Clock,
  Cloud,
  CloudCog,
  Database,
  FileClock,
  Gauge,
  GitBranch,
  HardDrive,
  HeartPulse,
  KeyRound,
  Layers,
  LayoutDashboard,
  Orbit,
  Radio,
  Route as RouteIcon,
  Server,
  Settings,
  ShieldCheck,
  Sparkles,
  Timer,
  Users,
  Wrench,
} from "lucide-react";
import type { NavRole } from "../lib/auth";
import Overview from "../views/Overview";
import Volumes from "../views/Volumes";
import Rbd from "../views/Rbd";
import Snapshots from "../views/Snapshots";
import Schedules from "../views/Schedules";
import Backups from "../views/Backups";
import Buckets from "../views/Buckets";
import ProtectionStatus from "../views/ProtectionStatus";
import Alerts from "../views/Alerts";
import ActivityView from "../views/Activity";
import Observatory from "../views/Observatory";
import Ceph from "../views/Ceph";
import PoolDetail from "../views/PoolDetail";
import Jobs from "../views/Jobs";
import Audit from "../views/Audit";
import Tenants from "../views/Tenants";
import Access from "../views/Access";
import SettingsView from "../views/Settings";
import ApiDocs from "../views/ApiDocs";
import { Backends, Cluster, Kubernetes, Metrics, Policies } from "../views/Simple";
import Maintenance from "../views/Maintenance";
import DR from "../views/DR";
import Sources from "../views/databridge/Sources";
import Plans from "../views/databridge/Plans";
import PlanDetail from "../views/databridge/PlanDetail";
import EdgeClusters from "../views/databridge/EdgeClusters";
import Replication from "../views/databridge/Replication";
import Validation from "../views/databridge/Validation";

export type SectionId =
  | "STORAGE"
  | "DATA PROTECTION"
  | "DATABRIDGE"
  | "OBSERVABILITY"
  | "GOVERNANCE"
  | "INFRASTRUCTURE";

export interface NavModule {
  id: string;
  codename: string;
  label: string;
  path: string;
  icon: LucideIcon;
  section: SectionId;
  minRole?: NavRole;
  pinned?: boolean;
  shortcut?: string;
  hiddenFromNav?: boolean;
  element?: ComponentType;
}

export const SECTIONS: SectionId[] = [
  "STORAGE",
  "DATA PROTECTION",
  "DATABRIDGE",
  "OBSERVABILITY",
  "GOVERNANCE",
  "INFRASTRUCTURE",
];

export const SECTION_META: Record<SectionId, { short: string; collapsible: boolean }> = {
  STORAGE: { short: "Storage", collapsible: false },
  "DATA PROTECTION": { short: "Protect", collapsible: true },
  DATABRIDGE: { short: "DataBridge", collapsible: true },
  OBSERVABILITY: { short: "Observe", collapsible: true },
  GOVERNANCE: { short: "Govern", collapsible: true },
  INFRASTRUCTURE: { short: "Infra", collapsible: true },
};

export const MODULES: NavModule[] = [
  {
    id: "overview",
    codename: "olympus",
    label: "Command Deck",
    path: "/",
    icon: LayoutDashboard,
    section: "STORAGE",
    pinned: true,
    shortcut: "H",
    element: Overview,
  },
  { id: "volumes", codename: "atlas", label: "Volumes", path: "/volumes", icon: HardDrive, section: "STORAGE", minRole: "operator", pinned: true, element: Volumes },
  { id: "rbd", codename: "hephaestus", label: "RBD Images", path: "/rbd", icon: Layers, section: "STORAGE", minRole: "operator", element: Rbd },
  { id: "snapshots", codename: "mnemosyne", label: "Snapshots", path: "/snapshots", icon: Camera, section: "STORAGE", minRole: "operator", element: Snapshots },
  { id: "schedules", codename: "chronos", label: "Schedules", path: "/schedules", icon: Timer, section: "STORAGE", minRole: "operator", element: Schedules },

  { id: "backups", codename: "hades", label: "Backups", path: "/backups", icon: Archive, section: "DATA PROTECTION", minRole: "operator", element: Backups },
  { id: "buckets", codename: "poseidon", label: "Buckets", path: "/buckets", icon: Cloud, section: "DATA PROTECTION", minRole: "operator", element: Buckets },
  { id: "protection", codename: "asclepius", label: "Protection Status", path: "/protection", icon: HeartPulse, section: "DATA PROTECTION", element: ProtectionStatus },

  { id: "cloud-databases", codename: "prometheus", label: "Cloud Databases", path: "/databridge/sources", icon: CloudCog, section: "DATABRIDGE", minRole: "operator", element: Sources },
  { id: "migration-plans", codename: "iris", label: "Migration Plans", path: "/databridge/plans", icon: RouteIcon, section: "DATABRIDGE", minRole: "operator", element: Plans },
  { id: "edge-clusters", codename: "epimetheus", label: "Edge DB Clusters", path: "/databridge/edge-clusters", icon: Database, section: "DATABRIDGE", minRole: "operator", element: EdgeClusters },
  { id: "replication", codename: "echo", label: "Replication", path: "/databridge/replication", icon: Radio, section: "DATABRIDGE", minRole: "operator", element: Replication },
  { id: "validation", codename: "astraea", label: "Validation", path: "/databridge/validation", icon: BadgeCheck, section: "DATABRIDGE", minRole: "operator", element: Validation },

  { id: "observatory", codename: "orrery", label: "Observatory", path: "/observatory", icon: Orbit, section: "OBSERVABILITY", pinned: true, element: Observatory },
  { id: "activity", codename: "kairos", label: "Activity", path: "/activity", icon: Activity, section: "OBSERVABILITY", element: ActivityView },
  { id: "alerts", codename: "hermes", label: "Alerts", path: "/alerts", icon: Bell, section: "OBSERVABILITY", minRole: "operator", pinned: true, element: Alerts },
  { id: "metrics", codename: "helios", label: "Metrics", path: "/metrics-dashboard", icon: Gauge, section: "OBSERVABILITY", element: Metrics },
  { id: "jobs", codename: "nike", label: "Jobs", path: "/jobs", icon: Clock, section: "OBSERVABILITY", minRole: "operator", pinned: true, element: Jobs },
  { id: "audit", codename: "themis", label: "Audit", path: "/audit", icon: FileClock, section: "OBSERVABILITY", element: Audit },

  { id: "tenants", codename: "athena", label: "Tenants", path: "/tenants", icon: Users, section: "GOVERNANCE", minRole: "admin", element: Tenants },
  { id: "access", codename: "aegis", label: "Access", path: "/access", icon: KeyRound, section: "GOVERNANCE", minRole: "admin", element: Access },
  { id: "policies", codename: "dike", label: "Policies", path: "/policies", icon: ShieldCheck, section: "GOVERNANCE", minRole: "admin", element: Policies },
  { id: "settings", codename: "hestia-ui", label: "Settings", path: "/settings", icon: Settings, section: "GOVERNANCE", minRole: "admin", pinned: true, element: SettingsView },
  { id: "api-docs", codename: "hermes-docs", label: "API Docs", path: "/api-docs", icon: BookOpen, section: "GOVERNANCE", element: ApiDocs },

  { id: "backends", codename: "gaia", label: "Backends", path: "/backends", icon: Server, section: "INFRASTRUCTURE", minRole: "admin", element: Backends },
  { id: "kubernetes", codename: "talos", label: "Kubernetes", path: "/kubernetes", icon: Boxes, section: "INFRASTRUCTURE", minRole: "operator", element: Kubernetes },
  { id: "cluster", codename: "oracle", label: "Cluster", path: "/cluster", icon: Database, section: "INFRASTRUCTURE", minRole: "operator", element: Cluster },
  { id: "ceph", codename: "kraken", label: "Ceph", path: "/ceph", icon: Aperture, section: "INFRASTRUCTURE", minRole: "operator", pinned: true, element: Ceph },
  { id: "maintenance", codename: "hestia", label: "Maintenance", path: "/maintenance", icon: Wrench, section: "INFRASTRUCTURE", minRole: "operator", element: Maintenance },
  { id: "dr", codename: "styx", label: "Disaster Recovery", path: "/dr", icon: GitBranch, section: "INFRASTRUCTURE", minRole: "admin", element: DR },

  { id: "pool-detail", codename: "pool", label: "Pool", path: "/pools/:id", icon: Database, section: "STORAGE", hiddenFromNav: true, element: PoolDetail },
  { id: "plan-detail", codename: "plan", label: "Plan", path: "/databridge/plans/:id", icon: RouteIcon, section: "DATABRIDGE", hiddenFromNav: true, element: PlanDetail },
];

export const SPARK = Sparkles;

/** Nav-visible modules filtered by role. */
export function modulesForRole(actorLevel: number): NavModule[] {
  return MODULES.filter((m) => !m.hiddenFromNav && canModuleAccess(m, actorLevel));
}

export function canModuleAccess(m: NavModule, actorLevel: number): boolean {
  const min = m.minRole ?? "viewer";
  const need = min === "admin" ? 2 : min === "operator" ? 1 : 0;
  return actorLevel >= need;
}

export function pinnedModules(actorLevel: number): NavModule[] {
  return modulesForRole(actorLevel).filter((m) => m.pinned);
}

/** Longest-prefix match for active nav item (h2kvm pattern). */
export function activeModuleFromPath(pathname: string): NavModule | undefined {
  if (pathname === "/") return MODULES.find((m) => m.path === "/");
  let best: { m: NavModule; len: number } | undefined;
  for (const m of MODULES) {
    if (m.path === "/") continue;
    if (pathname === m.path || pathname.startsWith(`${m.path}/`)) {
      if (!best || m.path.length > best.len) best = { m, len: m.path.length };
    }
  }
  return best?.m;
}

export function navLabelForPath(pathname: string): string | undefined {
  return activeModuleFromPath(pathname)?.label;
}

export function sectionForPath(pathname: string): SectionId | undefined {
  return activeModuleFromPath(pathname)?.section;
}

export const APP_ROUTES = MODULES.filter((m) => m.element);
