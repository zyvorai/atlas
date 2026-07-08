// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Module registry (mirrors Zeus OS osModules.ts shape) — single source for sidebar + dock + spotlight.
import type { LucideIcon } from "lucide-react";
import {
  Activity, Archive, Boxes, Camera, Clock, Cloud, Database, FileClock, Gauge, HardDrive,
  KeyRound, Layers, LayoutDashboard, Server, ShieldCheck, Sparkles, Timer, Users,
} from "lucide-react";

export interface Module {
  id: string;
  codename: string; // internal Greek flavor, Zeus-style
  label: string;
  path: string;
  icon: LucideIcon;
  section: string;
  dock?: boolean;
}

export const MODULES: Module[] = [
  { id: "overview", codename: "olympus", label: "Command Deck", path: "/", icon: LayoutDashboard, section: "STORAGE", dock: true },
  { id: "volumes", codename: "atlas", label: "Volumes", path: "/volumes", icon: HardDrive, section: "STORAGE", dock: true },
  { id: "rbd", codename: "hephaestus", label: "RBD Images", path: "/rbd", icon: Layers, section: "STORAGE" },
  { id: "snapshots", codename: "mnemosyne", label: "Snapshots", path: "/snapshots", icon: Camera, section: "STORAGE" },
  { id: "schedules", codename: "chronos", label: "Schedules", path: "/schedules", icon: Timer, section: "STORAGE" },

  { id: "backups", codename: "hades", label: "Backups", path: "/backups", icon: Archive, section: "DATA PROTECTION", dock: true },
  { id: "buckets", codename: "poseidon", label: "Buckets", path: "/buckets", icon: Cloud, section: "DATA PROTECTION" },

  { id: "alerts", codename: "hermes", label: "Alerts", path: "/alerts", icon: Activity, section: "OBSERVABILITY", dock: true },
  { id: "metrics", codename: "helios", label: "Metrics", path: "/metrics", icon: Gauge, section: "OBSERVABILITY" },
  { id: "jobs", codename: "nike", label: "Jobs", path: "/jobs", icon: Clock, section: "OBSERVABILITY", dock: true },
  { id: "audit", codename: "themis", label: "Audit", path: "/audit", icon: FileClock, section: "OBSERVABILITY" },

  { id: "tenants", codename: "athena", label: "Tenants", path: "/tenants", icon: Users, section: "GOVERNANCE" },
  { id: "access", codename: "aegis", label: "Access", path: "/access", icon: KeyRound, section: "GOVERNANCE" },
  { id: "policies", codename: "dike", label: "Policies", path: "/policies", icon: ShieldCheck, section: "GOVERNANCE" },

  { id: "backends", codename: "gaia", label: "Backends", path: "/backends", icon: Server, section: "INFRASTRUCTURE" },
  { id: "kubernetes", codename: "talos", label: "Kubernetes", path: "/kubernetes", icon: Boxes, section: "INFRASTRUCTURE" },
  { id: "cluster", codename: "oracle", label: "Cluster", path: "/cluster", icon: Database, section: "INFRASTRUCTURE" },
];

export const SECTIONS = ["STORAGE", "DATA PROTECTION", "OBSERVABILITY", "GOVERNANCE", "INFRASTRUCTURE"];
export const SPARK = Sparkles; // brand accent icon
