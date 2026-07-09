// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Client UI state (zustand): auth token, sidebar, spotlight, and the live job feed.
import { create } from "zustand";
import type { JobRecord } from "../api/types";

interface TrackedJob {
  id: string;
  label: string;
  state: string;
  progress: number;
  error?: string | null;
}

export type Theme = "nebula" | "dark" | "aurora";
export type Density = "comfortable" | "compact";

interface UiState {
  token: string;
  setToken: (t: string) => void;
  entered: boolean;
  enter: () => void;
  signOut: () => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
  density: Density;
  setDensity: (d: Density) => void;
  paused: boolean;
  togglePaused: () => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  spotlightOpen: boolean;
  setSpotlight: (v: boolean) => void;
  jobs: Record<string, TrackedJob>;
  trackJob: (id: string, label: string) => void;
  updateJob: (id: string, j: Partial<TrackedJob>) => void;
  clearDoneJobs: () => void;
}

const ls = typeof localStorage !== "undefined" ? localStorage : null;
const savedToken = ls?.getItem("atlas.token") || "";
const savedTheme = (ls?.getItem("atlas.theme") as Theme) || "nebula";
const savedDensity = (ls?.getItem("atlas.density") as Density) || "comfortable";

function applyTheme(t: Theme) {
  if (typeof document !== "undefined") document.documentElement.dataset.uiShell = t;
}
function applyDensity(d: Density) {
  if (typeof document !== "undefined") document.documentElement.dataset.density = d;
}
applyTheme(savedTheme);
applyDensity(savedDensity);

export const useUi = create<UiState>((set) => ({
  token: savedToken,
  setToken: (t) => {
    ls?.setItem("atlas.token", t);
    set({ token: t });
  },
  entered: ls?.getItem("atlas.entered") === "1",
  enter: () => {
    ls?.setItem("atlas.entered", "1");
    set({ entered: true });
  },
  signOut: () => {
    ls?.removeItem("atlas.entered");
    ls?.removeItem("atlas.token");
    set({ entered: false, token: "" });
  },
  theme: savedTheme,
  setTheme: (t) => {
    ls?.setItem("atlas.theme", t);
    applyTheme(t);
    set({ theme: t });
  },
  density: savedDensity,
  setDensity: (d) => {
    ls?.setItem("atlas.density", d);
    applyDensity(d);
    set({ density: d });
  },
  paused: false,
  togglePaused: () => set((s) => ({ paused: !s.paused })),
  sidebarCollapsed: false,
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  spotlightOpen: false,
  setSpotlight: (v) => set({ spotlightOpen: v }),
  jobs: {},
  trackJob: (id, label) =>
    set((s) => ({ jobs: { ...s.jobs, [id]: { id, label, state: "queued", progress: 0 } } })),
  updateJob: (id, j) =>
    set((s) => (s.jobs[id] ? { jobs: { ...s.jobs, [id]: { ...s.jobs[id], ...j } } } : s)),
  clearDoneJobs: () =>
    set((s) => {
      const jobs = { ...s.jobs };
      for (const k of Object.keys(jobs))
        if (jobs[k].state === "succeeded" || jobs[k].state === "failed") delete jobs[k];
      return { jobs };
    }),
}));

export function jobIsTerminal(state: string) {
  return state === "succeeded" || state === "failed";
}

export type { TrackedJob, JobRecord };
