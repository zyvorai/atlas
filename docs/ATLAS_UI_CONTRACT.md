# Atlas UI — Soundings design contract

Paste reference: `atlas-storage-center.html` (pixels). This section is **policy**.
If they disagree: reference wins on pixels; this file wins on rules.

## Identity

**Storage is terrain. Capacity is depth. Atlas is the chart.**

| Product | Base | Accent | Mono | Metaphor |
|---|---|---|---|---|
| PacketWolf | warm pelt-black | amber | JetBrains Mono | predator |
| Zeus OS | deep space navy | plasma violet | IBM Plex Mono | instrument deck |
| **Atlas** | **mineral void `#04070A`** | **survey cyan `#3FD0E8`** | **DM Mono** | **bathymetric chart** |

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
- Hairlines = cyan-tinted alpha (`--at-line*`), not white alpha.

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

Kit surfaces (`GlassSection`, `StatCard`, `SlideOver`, buttons, fields, badges) render in Soundings tokens under `.at-app`. Default shell theme is mineral void + survey cyan.

## Shell chrome (Zeus metal patterns)

Single metal **topbar** (brand · centered icon section menus · action cluster) follows Zeus OS
~Apr 2026 EnhancedLayout / `dark-steel` / `zinc-metal` (`f0360bae3`): square `barIcon` triggers,
hover/click flyouts, Look & feel menu (`LayoutTemplate`), icon search (⌘K).

| Atlas theme (`data-ui-shell`) | Look |
|---|---|
| **nebula** (default) | Metal chrome + **survey cyan** (Soundings identity) |
| **dark** | **Zeus dark-steel** (brushed metal + `#5d90f7` / `#8ec5ff`) |
| **zinc** | **Zeus zinc-metal** (brushed zinc + amber) |
| **aurora** | Neon cyan/violet/pink canvas |

Change Look & feel from the top-rail template icon after login (also on the sign-in page).
Nebula must not use steel-blue/amber as the default interaction colour; dark/zinc remap `--at-cyan*`.

## Ship checklist

Three-line header; health before inventory; no white-alpha hairlines; cluster strings in mono;
SI/binary compaction; no nested panels; zeros have fill hints; `depth(pct)` for fills;
⌘K / Esc / `:focus-visible`; responsive top nav at narrow widths (section menus).
