# Atlas UI — Soundings design contract

Paste reference: `atlas-storage-center.html` (pixels). This section is **policy**.
If they disagree: reference wins on pixels; this file wins on rules.

## Identity

**Storage is terrain. Capacity is depth. Atlas is the chart.**

| Product | Base | Accent | Mono | Metaphor |
|---|---|---|---|---|
| PacketWolf | warm pelt-black | amber | JetBrains Mono | predator |
| Zeus OS | deep space navy | plasma violet | IBM Plex Mono | instrument deck |
| **Atlas — Carbon** | **graphite `#0B0C0E`** | **Cosmic Orange `#F97316`** | **DM Mono** | **bathymetric chart** |
| **Atlas — Apple Lite** | **mist paper `#EDEFF3`** | **Mist Blue `#2A82B4`** | **DM Mono** | **bathymetric chart** |

Atlas ships exactly two shells, both ported 1:1 from Zeus OS (v9s): **Carbon** (Zeus's
`zyvor-carbon` — graphite + Cosmic Orange, default) and **Apple Lite** (Zeus's `tahoe-light` —
iPhone 17 Magichromatic: Mist Blue, Sage, Lavender, Cosmic Orange).

Tokens live in `crates/atlas-gateway/ui/src/atlas-soundings.css` (`--at-*`, `--d*`).

## Colour law

- **One interaction colour:** `--at-cyan` (buttons, focus, links, active nav).
- **Depth ramp is state, never interaction** — use `depth(pct)` from `lib/depth.ts`:

| Class | Range | Name |
|---|---|---|
| `.dp1` | `< 40%` | shoal |
| `.dp2` | `< 60%` | shelf |
| `.dp3` | `< 75%` | slope |
| `.dp4` | `< 90%` | deep |
| `.dp5` | `≥ 90%` | abyssal |

- Never put a depth colour on a button/link/nav.
- Amber/red (`--at-warn` / `--at-fail`) = cluster health verdicts, not capacity.
- Hairlines (`--at-line*`) are tinted by the shell's own ink colour, never a flat white or black
  alpha — Carbon's hairlines are neutral-light-tinted on graphite, Apple Lite's are ink-tinted on
  paper.

## Type law

- **Space Grotesk** — Atlas voice (titles, labels, prose, buttons).
- **DM Mono** — cluster voice (numbers, pool/volume names, paths, timestamps, health strings).
- Number anatomy: value → unit (`0.52em`, dimmer) → caption above.

## Number formatting

- SI compaction for ops counters (`299.4K`, `1.57M`) — no locale digit grouping.
- Bytes: binary units one decimal (`7.3 TiB`). Never mix TiB/TB.
- Separate read/write values; no subscript letter labels.
- Rates carry a window (`41 GiB/day`, `60 s · 1 s buckets`).

## Structure

- Max panel nesting depth **1**. Group with hairlines (`.at-lattice`), not card soup.
- Measured data is sharp (`--r-data: 0`); controls are round (`--r-ctl`).
- Page grammar: **eyebrow → title → state** (state names the real exception).
- Answer *is it healthy?* before *what is it?* before *what can I do?*

## Emptiness

- Naked `0` is banned — always a fill hint + action.

## Motion (only)

1. Chart floor drift (120s) 2. Echogram sweep (4.6s) 3. Depth fills (900ms) 4. Page surface (420ms).
Respect `prefers-reduced-motion`.

## Signatures (do not dilute)

Chart floor · The Sounding (no capacity donut) · Echogram · Basins · Seabed.

## Page archetypes

| Letter | Role | Ship first on |
|---|---|---|
| **A** Command Deck | Sounding + Echogram + Lattice + Basins | `/` Overview |
| **B** Index | chips + single `at-tbl` + hover row actions (+ depth where fill applies) | all inventory indexes |
| **C** Detail | 4-up instruments + cross-section + seabed + ledger | `/pools/:id`, PlanDetail pipeline |
| **D** Telemetry | one wide echogram + breakdowns | `/observatory` |
| **E** Ops | `at-instrs` / `at-stack` / Soundings kit remap | Ceph, Access, Maintenance, DR, Cluster |

Kit surfaces (`GlassSection`, `StatCard`, `SlideOver`, buttons, fields, badges) render in Soundings tokens under `.at-app`. Default shell theme is **Carbon** (graphite + Cosmic Orange).

## Shell chrome (apple.com topbar + Zeus OS sidebar)

Navigation lives in exactly one place — a **persistent left sidebar** (`Sidebar` in `Shell.tsx`,
Zeus OS's sidebar pattern): the six section groups (Storage, Data Protection, DataBridge,
Observability, Governance, Infrastructure) always visible as icon+label links, collapsible to a
64px icon-only rail (toggle at the bottom of the sidebar; state persisted as
`atlas.sidebar-collapsed`). Below ~900px the sidebar hides entirely and a hamburger button opens
`MobileNavDrawer` (the same grouped sections + shortcuts, as a slide-in panel) instead — the two
never show at once.

The **topbar** above it is deliberately sparse, apple.com-style — a bare 44px-tall (`--rail-h`)
translucent strip with just the brand mark (icon only, no wordmark) on the left and a slim
single-glyph icon cluster on the right: search (⌘K), Look & feel (`LayoutTemplate`), a running-jobs
spinner (conditional), alerts bell, pause/resume, a compact health dot+label, and one Account icon
(auth token / local time / sign-out combined). It carries no navigation and no live-metrics chips —
those either live in the sidebar or are one click away on the Command Deck.

| Atlas theme (`data-ui-shell`) | Look |
|---|---|
| **carbon** (default) | Zeus's `zyvor-carbon` — graphite (`#0B0C0E` / `#15171B`) + Cosmic Orange `#F97316` |
| **apple-lite** | Zeus's `tahoe-light` — mist paper (`#EDEFF3` / `#FFFFFF`) + Mist Blue `#2A82B4`, iPhone 17 Magichromatic |

Change Look & feel from the topbar icon after login, or **Settings → Appearance** (theme +
density). Density is stored as `data-density` (`comfortable` | `compact`). The sidebar's own
colors (`.at-sidebar*`) are built entirely from `--at-*` tokens — never literal white/black alpha —
so both shells render correctly with no per-shell CSS overrides needed for it.

**Not ported (by design):** Dock, Finder, Dynamic Island, VM theatre, and Zeus's fuller sidebar
features (auto-hide/peek, nav tiers, the "all apps" picker) — Atlas's sidebar is a simplified,
always-icon-plus-label-or-collapsed version scoped to the six section groups above.

Both shells keep flat Soundings panels (`.at-panel` is transparent + a top hairline, not a metal
card) — content panels are ruled, not card soup, per the Structure law below.

## Ship checklist

Three-line header; health before inventory; no white-alpha hairlines; cluster strings in mono;
SI/binary compaction; no nested panels; zeros have fill hints; `depth(pct)` for fills;
⌘K / Esc / `:focus-visible`; sidebar collapses to a drawer at narrow widths.
