// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import React from "react";
import { EmptyState, Spinner } from "./kit";

export type Col<T> = { h: string; f: (r: T) => React.ReactNode; mono?: boolean };

export function Table<T>({
  cols,
  rows,
  actions,
  onRow,
  empty = "Nothing here yet.",
  rowKey,
}: {
  cols: Col<T>[];
  rows: T[] | undefined;
  actions?: (r: T) => React.ReactNode;
  onRow?: (r: T) => void;
  empty?: string;
  rowKey?: (r: T, i: number) => string;
}) {
  if (!rows) return <Spinner />; // undefined = still loading
  if (!rows.length) return <EmptyState msg={empty} />;
  return (
    <div className="overflow-x-auto">
      <table className="ztable">
        <thead>
          <tr>
            {cols.map((c) => (
              <th key={c.h}>{c.h}</th>
            ))}
            {actions && <th />}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
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
