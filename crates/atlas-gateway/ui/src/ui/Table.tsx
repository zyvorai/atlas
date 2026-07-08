// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import React, { useMemo, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { EmptyState, Spinner } from "./kit";

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
}: {
  cols: Col<T>[];
  rows: T[] | undefined;
  actions?: (r: T) => React.ReactNode;
  onRow?: (r: T) => void;
  empty?: string;
  emptyCta?: React.ReactNode;
  rowKey?: (r: T, i: number) => string;
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

  if (!rows) return <Spinner />; // undefined = still loading
  if (!rows.length) return <EmptyState msg={empty} cta={emptyCta} />;
  const clickHeader = (i: number) => {
    if (!cols[i].sortKey) return;
    setSort((s) => (s?.i === i ? { i, dir: s.dir === 1 ? -1 : 1 } : { i, dir: 1 }));
  };
  return (
    <div className="overflow-x-auto">
      <table className="ztable">
        <thead>
          <tr>
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
          {(sorted || []).map((r, i) => (
            <tr
              key={rowKey ? rowKey(r, i) : i}
              onClick={onRow ? () => onRow(r) : undefined}
              className={onRow ? "cursor-pointer" : undefined}
            >
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
          ))}
        </tbody>
      </table>
    </div>
  );
}
