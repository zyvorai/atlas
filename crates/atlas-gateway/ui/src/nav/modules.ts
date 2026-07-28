// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Module registry — single source for top nav + spotlight.
import type { LucideIcon } from "lucide-react";
import {
  Activity, Aperture, Archive, Bell, BookOpen, Boxes, Camera, Clock, Cloud, Database, FileClock, Gauge, HardDrive,
  KeyRound, Layers, LayoutDashboard, Orbit, Server, Settings, ShieldCheck, Sparkles, Timer, Users, CloudCog,
  Route as RouteIcon, Radio, Wrench, GitBranch,
} from "lucide-react";

export interface Module {
  id: string;
  codename: string; // internal Greek flavor
  label: string;
  path: string;
  icon: LucideIcon;
  section: string;
}

/** Menubar shortcut controls — macOS 26–style Control Center chips pinned to the rail. */
export const MENUBAR_CONTROLS: (Module & { shortcut?: string })[] = [
  { id: "ctl-deck", codename: "olympus", label: "Deck", path: "/", icon: LayoutDashboard, section: "STORAGE", shortcut: "H" },
  { id: "ctl-volumes", codename: "atlas", label: "Volumes", path: "/volumes", icon: HardDrive, section: "STORAGE" },
  { id: "ctl-jobs", codename: "nike", label: "Jobs", path: "/jobs", icon: Clock, section: "OBSERVABILITY" },
  { id: "ctl-alerts", codename: "hermes", label: "Alerts", path: "/alerts", icon: Bell, section: "OBSERVABILITY" },
  { id: "ctl-ceph", codename: "kraken", label: "Ceph", path: "/ceph", icon: Aperture, section: "INFRASTRUCTURE" },
  { id: "ctl-observatory", codename: "orrery", label: "Observatory", path: "/observatory", icon: Orbit, section: "OBSERVABILITY" },
  { id: "ctl-settings", codename: "settings", label: "Settings", path: "/settings", icon: Settings, section: "GOVERNANCE" },
];

/** @deprecated Prefer MENUBAR_CONTROLS — kept for any external imports. */
export const TOP_BAR_QUICK_LINKS = MENUBAR_CONTROLS;

export const MODULES: Module[] = [
  { id: "overview", codename: "olympus", label: "Command Deck", path: "/", icon: LayoutDashboard, section: "STORAGE" },
  { id: "volumes", codename: "atlas", label: "Volumes", path: "/volumes", icon: HardDrive, section: "STORAGE" },
  { id: "rbd", codename: "hephaestus", label: "RBD Images", path: "/rbd", icon: Layers, section: "STORAGE" },
  { id: "snapshots", codename: "mnemosyne", label: "Snapshots", path: "/snapshots", icon: Camera, section: "STORAGE" },
  { id: "schedules", codename: "chronos", label: "Schedules", path: "/schedules", icon: Timer, section: "STORAGE" },

  { id: "backups", codename: "hades", label: "Backups", path: "/backups", icon: Archive, section: "DATA PROTECTION" },
  { id: "buckets", codename: "poseidon", label: "Buckets", path: "/buckets", icon: Cloud, section: "DATA PROTECTION" },

  { id: "observatory", codename: "orrery", label: "Observatory", path: "/observatory", icon: Orbit, section: "OBSERVABILITY" },
  { id: "activity", codename: "kairos", label: "Activity", path: "/activity", icon: Activity, section: "OBSERVABILITY" },
  { id: "alerts", codename: "hermes", label: "Alerts", path: "/alerts", icon: Bell, section: "OBSERVABILITY" },
  { id: "metrics", codename: "helios", label: "Metrics", path: "/metrics-dashboard", icon: Gauge, section: "OBSERVABILITY" },
  { id: "jobs", codename: "nike", label: "Jobs", path: "/jobs", icon: Clock, section: "OBSERVABILITY" },
  { id: "audit", codename: "themis", label: "Audit", path: "/audit", icon: FileClock, section: "OBSERVABILITY" },

  { id: "tenants", codename: "athena", label: "Tenants", path: "/tenants", icon: Users, section: "GOVERNANCE" },
  { id: "access", codename: "aegis", label: "Access", path: "/access", icon: KeyRound, section: "GOVERNANCE" },
  { id: "policies", codename: "dike", label: "Policies", path: "/policies", icon: ShieldCheck, section: "GOVERNANCE" },
  { id: "settings", codename: "hestia-ui", label: "Settings", path: "/settings", icon: Settings, section: "GOVERNANCE" },
  { id: "api-docs", codename: "hermes-docs", label: "API Docs", path: "/api-docs", icon: BookOpen, section: "GOVERNANCE" },

  { id: "cloud-databases", codename: "prometheus", label: "Cloud Databases", path: "/databridge/sources", icon: CloudCog, section: "DATABRIDGE" },
  { id: "migration-plans", codename: "iris", label: "Migration Plans", path: "/databridge/plans", icon: RouteIcon, section: "DATABRIDGE" },
  { id: "edge-clusters", codename: "epimetheus", label: "Edge DB Clusters", path: "/databridge/edge-clusters", icon: Server, section: "DATABRIDGE" },
  { id: "replication", codename: "echo", label: "Replication", path: "/databridge/replication", icon: Radio, section: "DATABRIDGE" },
  { id: "validation", codename: "astraea", label: "Validation", path: "/databridge/validation", icon: ShieldCheck, section: "DATABRIDGE" },

  { id: "backends", codename: "gaia", label: "Backends", path: "/backends", icon: Server, section: "INFRASTRUCTURE" },
  { id: "kubernetes", codename: "talos", label: "Kubernetes", path: "/kubernetes", icon: Boxes, section: "INFRASTRUCTURE" },
  { id: "cluster", codename: "oracle", label: "Cluster", path: "/cluster", icon: Database, section: "INFRASTRUCTURE" },
  { id: "ceph", codename: "kraken", label: "Ceph", path: "/ceph", icon: Aperture, section: "INFRASTRUCTURE" },
  { id: "maintenance", codename: "hestia", label: "Maintenance", path: "/maintenance", icon: Wrench, section: "INFRASTRUCTURE" },
  { id: "dr", codename: "styx", label: "Disaster Recovery", path: "/dr", icon: GitBranch, section: "INFRASTRUCTURE" },
];

export const SECTIONS = ["STORAGE", "DATA PROTECTION", "DATABRIDGE", "OBSERVABILITY", "GOVERNANCE", "INFRASTRUCTURE"];
export const SPARK = Sparkles; // brand accent icon
