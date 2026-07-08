// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Imperative confirm() dialog — `await confirm({...})` from anywhere; a single <ConfirmHost/> renders it.
import { useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { Button, Modal } from "./kit";

export interface ConfirmOpts {
  title?: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
}

let opener: ((o: ConfirmOpts) => Promise<boolean>) | null = null;

export function confirm(o: ConfirmOpts): Promise<boolean> {
  return opener ? opener(o) : Promise.resolve(window.confirm(o.message));
}

/** Confirm a destructive action, then run it. */
export async function confirmThen(o: ConfirmOpts, run: () => void) {
  if (await confirm(o)) run();
}
export const del = (what: string, run: () => void) =>
  confirmThen({ title: `Delete ${what}?`, message: "This action cannot be undone.", confirmLabel: "Delete", danger: true }, run);

export function ConfirmHost() {
  const [state, setState] = useState<{ o: ConfirmOpts; resolve: (v: boolean) => void } | null>(null);
  useEffect(() => {
    opener = (o) => new Promise<boolean>((resolve) => setState({ o, resolve }));
    return () => {
      opener = null;
    };
  }, []);
  if (!state) return null;
  const done = (v: boolean) => {
    state.resolve(v);
    setState(null);
  };
  return (
    <Modal
      open
      onClose={() => done(false)}
      title={
        <span className="flex items-center gap-2">
          <AlertTriangle size={16} className={state.o.danger ? "text-danger" : "text-warning"} />
          {state.o.title || "Confirm"}
        </span>
      }
      footer={
        <>
          <Button onClick={() => done(false)}>Cancel</Button>
          <Button variant={state.o.danger ? "danger" : "primary"} onClick={() => done(true)}>
            {state.o.confirmLabel || "Confirm"}
          </Button>
        </>
      }
    >
      <div className="text-sm text-muted-foreground">{state.o.message}</div>
    </Modal>
  );
}
