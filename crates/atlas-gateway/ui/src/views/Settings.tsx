// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useUi, type Density } from "../store/ui";
import { THEME_OPTIONS } from "../lib/themes";
import { PageHead } from "../ui/PageHead";
import { cx } from "../lib/format";

const DENSITY_OPTIONS: { id: Density; title: string; hint: string }[] = [
  { id: "comfortable", title: "Comfortable", hint: "Default spacing for charts and tables" },
  { id: "compact", title: "Compact", hint: "Tighter rows — more density on large fleets" },
];

export default function Settings() {
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const density = useUi((s) => s.density);
  const setDensity = useUi((s) => s.setDensity);

  return (
    <div className="at-stack">
      <PageHead
        eyebrow="CONSOLE · APPEARANCE"
        title="Settings"
        state="Look & feel and density for this browser. Carbon is the dark default; Apple Lite is the iPhone 17 light shell."
      />

      <div className="at-panel">
        <div className="at-panel-bar">
          <span className="at-caption">Appearance</span>
        </div>
        <div className="at-settings-body">
          <div className="at-settings-block">
            <div className="at-settings-label">Shell theme</div>
            <p className="at-settings-hint">
              Carbon (graphite + Cosmic Orange) is the Atlas default. Apple Lite is the iPhone 17
              Magichromatic light shell — Mist Blue, Sage, Lavender, Cosmic Orange.
            </p>
            <div className="at-choice-grid" role="radiogroup" aria-label="Shell theme">
              {THEME_OPTIONS.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  role="radio"
                  aria-checked={theme === opt.id}
                  className={cx("at-choice", theme === opt.id && "on")}
                  onClick={() => setTheme(opt.id)}
                >
                  <span className="at-choice-title">{opt.title}</span>
                  <span className="at-choice-hint">{opt.hint}</span>
                </button>
              ))}
            </div>
          </div>

          <div className="at-settings-block">
            <div className="at-settings-label">Density</div>
            <p className="at-settings-hint">Applies to tables and page padding in this console.</p>
            <div className="at-choice-grid at-choice-grid-2" role="radiogroup" aria-label="Density">
              {DENSITY_OPTIONS.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  role="radio"
                  aria-checked={density === opt.id}
                  className={cx("at-choice", density === opt.id && "on")}
                  onClick={() => setDensity(opt.id)}
                >
                  <span className="at-choice-title">{opt.title}</span>
                  <span className="at-choice-hint">{opt.hint}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
