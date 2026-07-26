// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// axios client for the Atlas REST API + SSE job watcher + toast/job glue.
import axios from "axios";
import { useUi } from "../store/ui";
import type { JobRecord } from "../api/types";

export const API_BASE = "/api/atlas/v1";

export const http = axios.create({ baseURL: API_BASE });

http.interceptors.request.use((cfg) => {
  const token = useUi.getState().token;
  if (token) cfg.headers.Authorization = `Bearer ${token}`;
  return cfg;
});

// A 401 means the gateway has ATLAS_AUTH_REQUIRED=1 and our stored token is missing, expired, or
// revoked. Without this, queries just fail silently forever (widgets stuck on their loading
// spinner) since `entered` in the ui store is a one-time flag, not a live auth check. Bounce back
// to the login gate instead, with one toast — every in-flight request 401s at once on a bad token,
// so de-dupe rather than firing a toast per request.
let unauthorizedHandled = false;
http.interceptors.response.use(
  (res) => res,
  (err) => {
    if (err?.response?.status === 401 && !unauthorizedHandled) {
      unauthorizedHandled = true;
      const hadToken = !!useUi.getState().token;
      useUi.getState().signOut();
      toast(hadToken ? "Session expired — please re-enter your service-account token." : "Sign-in required.", "err");
      setTimeout(() => {
        unauthorizedHandled = false;
      }, 3000);
    }
    return Promise.reject(err);
  },
);

// Normalize the gateway's `{error:{message}}` envelope into a readable Error.
export function apiError(e: unknown): string {
  const anyE = e as { response?: { data?: { error?: { message?: string } } }; message?: string };
  return anyE?.response?.data?.error?.message || anyE?.message || "request failed";
}

export function isUnauthorized(e: unknown): boolean {
  return (e as { response?: { status?: number } })?.response?.status === 401;
}

// A toast bus (subscribed by <Toaster/>).
type Toast = { id: number; kind: "info" | "ok" | "err"; msg: string };
let toastId = 1;
const toastSubs = new Set<(t: Toast) => void>();
export function toast(msg: string, kind: Toast["kind"] = "info") {
  const t = { id: toastId++, kind, msg };
  toastSubs.forEach((f) => f(t));
}
export function onToast(fn: (t: Toast) => void): () => void {
  toastSubs.add(fn);
  return () => {
    toastSubs.delete(fn);
  };
}

/**
 * Track a job to completion via the SSE endpoint, updating the global job store
 * and toasting on terminal state. Falls back to polling if EventSource fails.
 */
export function watchJob(jobId: string, label: string, onDone?: () => void) {
  const ui = useUi.getState();
  ui.trackJob(jobId, label);
  toast(`${label} queued…`, "info");

  let settled = false;
  const finish = (state: string, error?: string | null) => {
    if (settled) return;
    settled = true;
    useUi.getState().updateJob(jobId, { state, progress: state === "succeeded" ? 100 : undefined, error });
    toast(state === "succeeded" ? `${label} ✓` : `${label} ✗ ${error || "failed"}`, state === "succeeded" ? "ok" : "err");
    onDone?.();
    setTimeout(() => useUi.getState().clearDoneJobs(), 4000);
  };

  try {
    const es = new EventSource(`${API_BASE}/jobs/${jobId}/watch`);
    es.addEventListener("job", (ev) => {
      try {
        const j = JSON.parse((ev as MessageEvent).data) as JobRecord;
        useUi.getState().updateJob(jobId, { state: j.state, progress: j.progress_percent, error: j.error });
        if (j.state === "succeeded" || j.state === "failed") {
          es.close();
          finish(j.state, j.error);
        }
      } catch {
        /* ignore */
      }
    });
    es.addEventListener("error", () => {
      es.close();
      // SSE dropped — poll a few times as a fallback.
      pollJob(jobId, label, finish);
    });
  } catch {
    pollJob(jobId, label, finish);
  }
}

async function pollJob(jobId: string, _label: string, finish: (s: string, e?: string | null) => void) {
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 1500));
    try {
      const { data } = await http.get<JobRecord>(`/jobs/${jobId}`);
      useUi.getState().updateJob(jobId, { state: data.state, progress: data.progress_percent, error: data.error });
      if (data.state === "succeeded" || data.state === "failed") return finish(data.state, data.error);
    } catch {
      /* keep trying */
    }
  }
  // Give up after 90s of polling — without this the job stays "running" in the UI forever with
  // no toast and no way to tell it's not actually being tracked anymore.
  finish("failed", "Timed out waiting for job status");
}

// Extract the job id from a 202 response body (varies by endpoint).
export function jobIdOf(resp: unknown): string | undefined {
  const r = resp as { job_id?: string; resource?: { job_id?: string } };
  return r?.job_id || r?.resource?.job_id;
}

/** Fire a write request; if it returns a job id, watch it to completion. Toasts + invalidates. */
export async function submitJob(
  method: "post" | "put" | "delete",
  path: string,
  body: unknown,
  label: string,
  after?: () => void,
) {
  try {
    const { data } = await http.request({ method, url: path, data: body });
    const jid = jobIdOf(data);
    if (jid) watchJob(jid, label, after);
    else {
      toast(`${label} ✓`, "ok");
      after?.();
    }
    return data;
  } catch (e) {
    // A 401 already gets its own explanation + login redirect from the response interceptor above;
    // piling on a second, request-specific toast ("create: invalid token: InvalidToken") just as the
    // screen is about to change is confusing rather than helpful.
    if (!isUnauthorized(e)) toast(`${label}: ${apiError(e)}`, "err");
    throw e;
  }
}

/** A plain write (no job), with toast + invalidate. */
export async function submit(method: "post" | "put" | "delete", path: string, body: unknown, label: string, after?: () => void) {
  try {
    const { data } = await http.request({ method, url: path, data: body });
    toast(`${label} ✓`, "ok");
    after?.();
    return data;
  } catch (e) {
    if (!isUnauthorized(e)) toast(`${label}: ${apiError(e)}`, "err");
    throw e;
  }
}
