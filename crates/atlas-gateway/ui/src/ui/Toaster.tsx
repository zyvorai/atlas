// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useEffect, useState } from "react";
import { CheckCircle2, Info, XCircle } from "lucide-react";
import { onToast } from "../api/client";

type T = { id: number; kind: "info" | "ok" | "err"; msg: string };

export function Toaster() {
  const [items, setItems] = useState<T[]>([]);
  useEffect(
    () =>
      onToast((t) => {
        setItems((s) => [...s, t]);
        setTimeout(() => setItems((s) => s.filter((x) => x.id !== t.id)), 4500);
      }),
    [],
  );
  return (
    // bottom-20 (not bottom-4) clears the floating bottom nav dock, which is horizontally centered
    // and at tablet widths sits close enough to the right edge to overlap a bottom-right toast.
    <div className="fixed right-4 bottom-20 z-[60] flex flex-col gap-2 w-[300px]">
      {items.map((t) => (
        <div
          key={t.id}
          className="glass-card px-3.5 py-2.5 flex items-center gap-2.5 text-sm animate-fade-in"
          style={{ borderLeft: `3px solid ${t.kind === "ok" ? "#30D69E" : t.kind === "err" ? "#E23B3B" : "#38BDF8"}` }}
        >
          {t.kind === "ok" ? (
            <CheckCircle2 size={16} className="text-success shrink-0" />
          ) : t.kind === "err" ? (
            <XCircle size={16} className="text-danger shrink-0" />
          ) : (
            <Info size={16} className="text-sky-400 shrink-0" />
          )}
          <span className="min-w-0">{t.msg}</span>
        </div>
      ))}
    </div>
  );
}
