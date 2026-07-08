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

export type Theme = "dark" | "aurora";

interface UiState {
  token: string;
  setToken: (t: string) => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
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
const savedTheme = (ls?.getItem("atlas.theme") as Theme) || "dark";

function applyTheme(t: Theme) {
  if (typeof document !== "undefined") document.documentElement.dataset.uiShell = t;
}
applyTheme(savedTheme);

export const useUi = create<UiState>((set) => ({
  token: savedToken,
  setToken: (t) => {
    ls?.setItem("atlas.token", t);
    set({ token: t });
  },
  theme: savedTheme,
  setTheme: (t) => {
    ls?.setItem("atlas.theme", t);
    applyTheme(t);
    set({ theme: t });
  },
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
