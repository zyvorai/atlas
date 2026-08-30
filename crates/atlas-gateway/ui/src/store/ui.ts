// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Client UI state (zustand): auth token, spotlight, and the live job feed.
import { create } from "zustand";
import type { JobRecord } from "../api/types";

interface TrackedJob {
  id: string;
  label: string;
  state: string;
  progress: number;
  error?: string | null;
}

export type Theme = "carbon" | "apple-lite";
export type Density = "comfortable" | "compact";

interface UiState {
  token: string;
  setToken: (t: string, persist?: boolean) => void;
  entered: boolean;
  enter: (persist?: boolean) => void;
  /** Optional soft hint shown on the login page after a forced sign-out (no toast). */
  sessionHint: string | null;
  clearSessionHint: () => void;
  signOut: (hint?: string) => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
  density: Density;
  setDensity: (d: Density) => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  paused: boolean;
  togglePaused: () => void;
  spotlightOpen: boolean;
  setSpotlight: (v: boolean) => void;
  jobs: Record<string, TrackedJob>;
  trackJob: (id: string, label: string) => void;
  updateJob: (id: string, j: Partial<TrackedJob>) => void;
  clearDoneJobs: () => void;
}

const ls = typeof localStorage !== "undefined" ? localStorage : null;
const ss = typeof sessionStorage !== "undefined" ? sessionStorage : null;

// The Login screen's "Remember token on this device" checkbox is a promise that an unchecked
// session doesn't survive the browser closing. `localStorage` alone can't keep that promise (it's
// unconditionally durable), so an unremembered sign-in lives in `sessionStorage` instead — a fresh
// tab session must never see a token/entered flag neither of us asked to persist.
const savedToken = ls?.getItem("atlas.token") || ss?.getItem("atlas.token") || "";
const savedEntered = ls?.getItem("atlas.entered") === "1" || ss?.getItem("atlas.entered") === "1";
const THEMES = new Set<Theme>(["carbon", "apple-lite"]);
const rawTheme = ls?.getItem("atlas.theme") || "";
const savedTheme: Theme = THEMES.has(rawTheme as Theme) ? (rawTheme as Theme) : "carbon";
const savedDensity = (ls?.getItem("atlas.density") as Density) || "comfortable";
const savedSidebarCollapsed = ls?.getItem("atlas.sidebar-collapsed") === "1";

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
  setToken: (t, persist = true) => {
    (persist ? ls : ss)?.setItem("atlas.token", t);
    (persist ? ss : ls)?.removeItem("atlas.token");
    set({ token: t });
  },
  entered: savedEntered,
  sessionHint: null as string | null,
  clearSessionHint: () => set({ sessionHint: null }),
  enter: (persist = true) => {
    (persist ? ls : ss)?.setItem("atlas.entered", "1");
    (persist ? ss : ls)?.removeItem("atlas.entered");
    set({ entered: true, sessionHint: null });
  },
  signOut: (hint) => {
    ls?.removeItem("atlas.entered");
    ls?.removeItem("atlas.token");
    ss?.removeItem("atlas.entered");
    ss?.removeItem("atlas.token");
    set({ entered: false, token: "", sessionHint: hint ?? null });
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
  sidebarCollapsed: savedSidebarCollapsed,
  toggleSidebar: () =>
    set((s) => {
      const next = !s.sidebarCollapsed;
      ls?.setItem("atlas.sidebar-collapsed", next ? "1" : "0");
      return { sidebarCollapsed: next };
    }),
  paused: false,
  togglePaused: () => set((s) => ({ paused: !s.paused })),
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
