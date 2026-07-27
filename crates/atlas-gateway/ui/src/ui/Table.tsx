// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import React, { useEffect, useMemo, useState } from "react";
import { AlertTriangle, ChevronDown, ChevronUp } from "lucide-react";
import { Button, EmptyState, Spinner } from "./kit";

export type Col<T> = {
  h: string;
  f: (r: T) => React.ReactNode;
  mono?: boolean;
  sortKey?: (r: T) => string | number;
};

export function Table<T>({
  cols,
  rows,
  actions,
  onRow,
  empty = "Nothing here yet.",
  emptyCta,
  rowKey,
  selectable,
  selected,
  onToggle,
  onToggleAll,
  error,
  onRetry,
}: {
  cols: Col<T>[];
  rows: T[] | undefined;
  actions?: (r: T) => React.ReactNode;
  onRow?: (r: T) => void;
  empty?: string;
  emptyCta?: React.ReactNode;
  rowKey?: (r: T, i: number) => string;
  selectable?: boolean;
  selected?: Set<string>;
  onToggle?: (key: string) => void;
  onToggleAll?: (keys: string[]) => void;
  /** True when the query backing `rows` failed — shows an error state instead of spinning forever
      (react-query leaves `data` undefined on both "still loading" and "errored", so callers must
      pass their hook's `isError` through here to tell the two apart). */
  error?: boolean;
  /** Refetch the failed query — typically the hook's own `refetch`. Without this the error state's
      "Try refreshing" is just text; a page-level "Refresh" button elsewhere (if any) usually refetches
      a *different* endpoint and won't actually retry this one. */
  onRetry?: () => void;
}) {
  const [sort, setSort] = useState<{ i: number; dir: 1 | -1 } | null>(null);
  const sorted = useMemo(() => {
    if (!rows || !sort) return rows;
    const key = cols[sort.i]?.sortKey;
    if (!key) return rows;
    return [...rows].sort((a, b) => {
      const ka = key(a);
      const kb = key(b);
      return (ka < kb ? -1 : ka > kb ? 1 : 0) * sort.dir;
    });
  }, [rows, sort, cols]);

  // Defense in depth: a query can fail in ways `error` never observes (e.g. a request that never
  // settles instead of properly rejecting) — `data`/`isError` staying exactly as they were on
  // mount looks identical to "still loading" and would spin forever. If loading drags on this long
  // something's wrong regardless of why, so fall back to the same error affordance.
  const [stuck, setStuck] = useState(false);
  useEffect(() => {
    if (rows) { setStuck(false); return; }
    const t = setTimeout(() => setStuck(true), 10000);
    return () => clearTimeout(t);
  }, [rows]);

  if (!rows) {
    if (error || stuck) {
      return (
        <div className="py-10 text-center text-sm text-danger flex flex-col items-center gap-2">
          <AlertTriangle size={18} />
          <span>Failed to load — the request errored.</span>
          {onRetry ? <Button size="sm" onClick={onRetry} className="mt-1">Retry</Button> : <span>Try refreshing.</span>}
        </div>
      );
    }
    return <Spinner />; // undefined = still loading
  }
  if (!rows.length) return <EmptyState msg={empty} cta={emptyCta} />;
  const clickHeader = (i: number) => {
    if (!cols[i].sortKey) return;
    setSort((s) => (s?.i === i ? { i, dir: s.dir === 1 ? -1 : 1 } : { i, dir: 1 }));
  };
  const rk = (r: T, i: number) => (rowKey ? rowKey(r, i) : String(i));
  const allKeys = (sorted || []).map((r, i) => rk(r, i));
  const allSelected = selectable && allKeys.length > 0 && allKeys.every((k) => selected?.has(k));
  return (
    <div className="overflow-x-auto">
      <table className="ztable">
        <thead>
          <tr>
            {selectable && (
              <th style={{ width: 34 }}>
                <input type="checkbox" checked={!!allSelected} onChange={() => onToggleAll?.(allKeys)} />
              </th>
            )}
            {cols.map((c, i) => (
              <th
                key={c.h}
                onClick={() => clickHeader(i)}
                className={c.sortKey ? "cursor-pointer select-none hover:text-white" : ""}
              >
                <span className="inline-flex items-center gap-1">
                  {c.h}
                  {sort?.i === i && (sort.dir === 1 ? <ChevronUp size={12} /> : <ChevronDown size={12} />)}
                </span>
              </th>
            ))}
            {actions && <th />}
          </tr>
        </thead>
        <tbody>
          {(sorted || []).map((r, i) => {
            const key = rk(r, i);
            return (
            <tr
              key={key}
              onClick={onRow ? () => onRow(r) : undefined}
              className={`${onRow ? "cursor-pointer" : ""} ${selected?.has(key) ? "selected" : ""}`}
            >
              {selectable && (
                <td onClick={(e) => e.stopPropagation()}>
                  <input type="checkbox" checked={selected?.has(key) || false} onChange={() => onToggle?.(key)} />
                </td>
              )}
              {cols.map((c) => (
                <td key={c.h} className={c.mono ? "mono" : undefined}>
                  {c.f(r) ?? "—"}
                </td>
              ))}
              {actions && (
                <td onClick={(e) => e.stopPropagation()}>
                  <div className="flex gap-1.5 justify-end">{actions(r)}</div>
                </td>
              )}
            </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
