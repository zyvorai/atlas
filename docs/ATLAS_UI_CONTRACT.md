# Atlas UI — Apple Shop design contract

Paste reference: Apple buy configurator (e.g. apple.com shop product pages). This section is **policy**.
If they disagree: this file wins on rules; shop pages win on pixels for boxes / type / swipe.

## Identity

**Storage is a product. Capacity is a choice. Atlas is the shop.**

| Product | Base | Accent | Mono | Metaphor |
|---|---|---|---|---|
| PacketWolf | warm pelt-black | amber | JetBrains Mono | predator |
| Zeus OS | deep space navy | plasma violet | IBM Plex Mono | instrument deck |
| **Atlas — Carbon** | **black `#000000`** | **Apple Blue light `#2997FF`** | **SF Mono stack** | **dark shop** |
| **Atlas — Apple Lite** | **shop gray `#F5F5F7`** | **Apple Blue `#0071E3`** | **SF Mono stack** | **light shop** |

Atlas ships exactly two shells: **Carbon** (dark Apple store) and **Apple Lite** (classic light shop).
Cosmic Orange is reserved for the brand mark only — never for CTAs or selected tiles.

Tokens live in `crates/atlas-gateway/ui/src/atlas-soundings.css` (`--at-*`, `--d*`).

## Colour law

- **One interaction colour:** `--at-cyan` (buttons, focus, links, active nav, selected tiles).
  - Carbon: `#2997FF`. Apple Lite: `#0071E3`.
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
- Borders (`--at-line*`) are soft shop edges on elevated boxes — never harsh ruled hairlines as the primary grouping language.

## Type law

- **SF system stack** — Atlas voice (titles, labels, prose, buttons):
  `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif`.
- **SF Mono stack** — cluster voice (numbers, pool/volume names, paths, timestamps, health strings):
  `ui-monospace, SFMono-Regular, Menlo, monospace`.
- Number anatomy: value → unit (`0.52em`, dimmer) → caption above.
- Titles are large shop display weight; subtitles are muted `#6e6e73` / `#a1a1a6`.

## Number formatting

- SI compaction for ops counters (`299.4K`, `1.57M`) — no locale digit grouping.
- Bytes: binary units one decimal (`7.3 TiB`). Never mix TiB/TB.
- Separate read/write values; no subscript letter labels.
- Rates carry a window (`41 GiB/day`, `60 s · 1 s buckets`).

## Structure

- Max panel nesting depth **1**. Group with **rounded elevated boxes** (`.at-panel`, `.at-card`, `.at-instr`), not hairline lattices.
- Boxes: radius **18–22px** (`--r-panel`), soft shadow on light shell, elevated fill on dark shell.
- **Selection tiles** (`.at-select-tile`): rounded box + 1px border; selected = 2px accent border.
- Measured data can stay sharp inside boxes (`--r-data`); controls are round (`--r-ctl` / pills).
- Page grammar: **eyebrow → title → state** (state names the real exception).
- Answer *is it healthy?* before *what is it?* before *what can I do?*

## Emptiness

- Naked `0` is banned — always a fill hint + action, preferably inside a large centered shop box.

## Motion

1. **Swipe rail** (scroll-snap-x + chevrons + touch/trackpad) for feature / status strips.
2. Depth fills (900ms).
3. Page surface (420ms).
Respect `prefers-reduced-motion` — swipe rails fall back to a static wrap.

## Signatures (do not dilute)

Shop canvas · Elevated boxes · Selection tiles · SwipeRail · SF type · Apple Blue CTAs.

## Page archetypes

| Letter | Role | Ship first on |
|---|---|---|
| **A** Shop Deck | Hero + SwipeRail of status tiles + elevated boxes | `/` Overview |
| **B** Index | shop pills + single table-in-box + empty box | all inventory indexes |
| **C** Detail | 4-up instrument boxes + swipe stage rail | `/pools/:id`, PlanDetail |
| **D** Telemetry | charts inside shop boxes (+ optional swipe modes) | `/observatory` |
| **E** Ops | equal rounded action boxes in a grid | Ceph, Access, Maintenance, DR, Cluster |

Kit surfaces (`GlassSection`, `StatCard`, `SwipeRail`, `SelectTile`, `SlideOver`, buttons, fields, badges) render in shop tokens under `.at-app`. Default shell theme is **Carbon** (dark shop).

## Shell chrome (shop topbar + sidebar)

Navigation lives in exactly one place — a **persistent left sidebar** (`Sidebar` in `Shell.tsx`):
six section groups (Storage, Data Protection, DataBridge, Observability, Governance,
Infrastructure). **Storage** is always expanded; other sections are **collapsible** (chevron
header, state in `atlas.sidebar-section-<id>`). The section with the active route stays open.
A **Filter navigation…** field at the top of the sidebar filters items across sections (hidden
on the icon-only rail). The whole sidebar collapses to a 64px icon rail (toggle at the bottom;
`atlas.sidebar-collapsed`). Below ~900px the sidebar hides and a hamburger opens
`MobileNavDrawer` with the same filter + sections — no duplicate shortcut list.

**Role-aware nav:** JWT role (`viewer` | `operator` | `admin`) filters sidebar and Spotlight
entries by `minRole` on each module in `nav/routes.ts`. Gated routes show an access-denied
panel if opened directly.

The **topbar** is a bare 44px (`--rail-h`) frosted shop strip: Zyvor mark + **Atlas** left,
icon cluster right (⌘K, Look & feel, jobs, alerts, pause, health, account). No nav chips in
the topbar. Materials: frosted light bar on Apple Lite, dark translucent bar on Carbon; active
states use Apple Blue.

**Detail pages** use `PageHead` breadcrumbs (`Storage · Pools · …`, `DataBridge · Migration Plans · …`).

| Atlas theme (`data-ui-shell`) | Look |
|---|---|
| **carbon** (default) | Dark shop — canvas `#000`, elevated boxes `#1D1D1F`, accent `#2997FF` |
| **apple-lite** | Light shop — canvas `#F5F5F7`, boxes `#FFF`, accent `#0071E3` |

Change Look & feel from the topbar icon after login, or **Settings → Appearance** (theme +
density). Density is stored as `data-density` (`comfortable` | `compact`).

## Ship checklist

Three-line header; health before inventory; elevated rounded boxes; SF type; Apple Blue CTAs;
selection tiles for config choices; SwipeRail where strips scroll; SI/binary compaction;
no nested panels; zeros have fill hints; `depth(pct)` for fills; ⌘K / Esc / `:focus-visible`;
sidebar collapses to a drawer at narrow widths; `prefers-reduced-motion` respected.
