// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import type { ReactNode } from "react";

/** Soundings page grammar: eyebrow → title → state (state names the real exception). */
export function PageHead({
  eyebrow,
  title,
  state,
  actions,
}: {
  eyebrow: ReactNode;
  title: ReactNode;
  state?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="at-head">
      <div>
        <div className="at-eyebrow">
          <span className="tick" />
          {eyebrow}
        </div>
        <h1 className="at-title">{title}</h1>
        {state != null && state !== false ? <p className="at-state">{state}</p> : null}
      </div>
      {actions ? <div className="at-head-actions">{actions}</div> : null}
    </div>
  );
}
