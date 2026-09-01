// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { NavLink } from "react-router-dom";
import { ChevronDown, Search } from "lucide-react";
import type { NavGroup } from "../nav/useNavGroups";
import { cx } from "../lib/format";

export function NavFilter({
  value,
  onChange,
  className,
}: {
  value: string;
  onChange: (v: string) => void;
  className?: string;
}) {
  return (
    <label className={cx("at-sidebar-filter", className)}>
      <span className="sr-only">Filter navigation</span>
      <Search className="at-sidebar-filter-icon" size={14} aria-hidden />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Filter navigation…"
        className="at-sidebar-filter-input"
        autoComplete="off"
      />
    </label>
  );
}

export function NavSectionLinks({
  groups,
  isSectionClosed,
  onToggleSection,
  linkClass,
  iconSize = 16,
  showSectionHeaders = true,
  compact = false,
  onNavigate,
}: {
  groups: NavGroup[];
  isSectionClosed: (g: NavGroup) => boolean;
  onToggleSection: (sec: NavGroup["sec"]) => void;
  linkClass: (active: boolean) => string;
  iconSize?: number;
  showSectionHeaders?: boolean;
  compact?: boolean;
  onNavigate?: () => void;
}) {
  return (
    <>
      {groups.map((g, groupIdx) => {
        const closed = isSectionClosed(g);
        return (
          <div key={g.sec} className="at-sidebar-group">
            {showSectionHeaders &&
              (g.collapsible ? (
                <button
                  type="button"
                  className="at-sidebar-section-header"
                  onClick={() => onToggleSection(g.sec)}
                  aria-expanded={!closed}
                >
                  <ChevronDown className={cx("at-sidebar-chevron", closed && "is-closed")} size={14} aria-hidden />
                  <span>{g.short}</span>
                </button>
              ) : (
                <div className="at-sidebar-label">{g.short}</div>
              ))}
            {!closed &&
              g.items.map((m) => (
                <NavLink
                  key={m.id}
                  to={m.path}
                  end={m.path === "/"}
                  title={m.label}
                  aria-label={m.label}
                  className={({ isActive }) => linkClass(isActive)}
                  onClick={onNavigate}
                >
                  <m.icon size={iconSize} strokeWidth={2} aria-hidden />
                  {!compact && <span>{m.label}</span>}
                  {!compact && m.shortcut ? <kbd className="at-control-kbd">{m.shortcut}</kbd> : null}
                </NavLink>
              ))}
            {showSectionHeaders && groupIdx < groups.length - 1 && !closed ? (
              <div className="at-sidebar-divider" aria-hidden />
            ) : null}
          </div>
        );
      })}
    </>
  );
}
