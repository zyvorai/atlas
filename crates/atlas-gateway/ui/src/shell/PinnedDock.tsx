// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Slim pinned dock — Atlas analog of h2kvm DesktopDock (no magnification). */
import { useNavigate, useLocation } from "react-router-dom";
import { Search } from "lucide-react";
import { pinnedModules, activeModuleFromPath } from "../nav/routes";
import { NavAppIcon } from "../nav/NavAppIcon";
import { useUi } from "../store/ui";
import { cx } from "../lib/format";
import { isModuleActive } from "../nav/useNavGroups";

export function PinnedDock({ onSpotlight, onLaunchPad }: { onSpotlight: () => void; onLaunchPad: () => void }) {
  const nav = useNavigate();
  const loc = useLocation();
  const roleLevel = useUi((s) => s.roleLevel);
  const pinned = pinnedModules(roleLevel);
  const active = activeModuleFromPath(loc.pathname);

  return (
    <nav className="at-dock" aria-label="Pinned">
      <div className="at-dock-shelf">
        {pinned.map((m) => {
          const on = isModuleActive(m, loc.pathname) || active?.id === m.id;
          return (
            <button
              key={m.id}
              type="button"
              className={cx("at-dock-tile", on && "on")}
              title={m.label}
              aria-label={m.label}
              aria-current={on ? "page" : undefined}
              onClick={() => nav(m.path)}
            >
              <NavAppIcon icon={m.icon} section={m.section} size={18} />
              <span className="at-dock-tip">{m.label}</span>
            </button>
          );
        })}
        <span className="at-dock-sep" aria-hidden />
        <button type="button" className="at-dock-tile" title="Search" aria-label="Search" onClick={onSpotlight}>
          <span className="at-appicon at-appicon-observe">
            <Search size={18} strokeWidth={2} />
          </span>
          <span className="at-dock-tip">Search</span>
        </button>
        <button
          type="button"
          className="at-dock-tile"
          title="Launch pad"
          aria-label="Launch pad"
          onClick={onLaunchPad}
        >
          <span className="at-appicon at-appicon-storage at-dock-grid" aria-hidden>
            <span /><span /><span /><span />
          </span>
          <span className="at-dock-tip">Launch pad</span>
        </button>
      </div>
    </nav>
  );
}
