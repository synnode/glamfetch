# Glamfetch — Technical Specification (v1 / MVP)

> A glamorous system-info fetch tool for the terminal.
> Built for rice enthusiasts who want full styling control without sacrificing speed.

---

## 1. Overview

**Glamfetch** is a system information fetch tool written in Rust, positioned as an *alternative* to `fastfetch` / `neofetch` — not a replacement. It targets users who want:

- **First-class theming and styling** (gradients, borders, gauges, sparklines)
- **A real grid layout system** (rows × columns, not just `logo + info`)
- **Composable widgets** decoupled from data sources
- **Fastfetch-class startup time** (target: < 50ms cold, < 20ms typical)
- **Hot-reloadable config** for live editing

The tool runs once and exits by default (like fastfetch), with optional `--watch` (refresh loop) and `--edit` (split-pane live preview) modes.

---

## 2. Philosophy & Non-Goals

### Core principles
1. **Data layer ≠ render layer.** Collectors produce *typed data*. Widgets consume data and produce *rendered cells*. Layout composes cells. These three layers must never be coupled.
2. **Composability over completeness.** A small set of orthogonal widgets that combine well > a large set of pre-built modules.
3. **Sane defaults, infinite customisation.** A user with zero config gets something pretty; a user with 200 lines of config gets exactly what they envisioned.
4. **Performance is a feature.** Anything > 100ms feels sluggish. Stay well under.

### Non-goals (v1)
- Not a real-time system monitor (that's `btop`'s job)
- Not a fastfetch drop-in (no config compat, no module-name compat)
- No image rendering (kitty/sixel/chafa) in v1 — ASCII + figlet only
- No plugin system / Lua / scripting in v1
- No support for non-Linux systems in v1 (macOS/BSD can be added later by abstracting collectors)

---

## 3. Tech Stack

| Concern | Choice | Rationale |
|---|---|---|
| Language | Rust 2021 edition | Performance + ecosystem fit for Michael's stack |
| CLI parsing | `clap` v4 (derive) | Standard, low overhead |
| Config format | TOML via `serde` + `toml` | Familiar to all dev audiences |
| Parallel collection | `rayon` | Drop-in `par_iter` |
| Syscalls | `rustix` (preferred) or `nix` | Direct kernel interfaces, no subprocess |
| Terminal | `crossterm` | TTY detection, raw mode for `--watch`/`--edit` |
| Width measurement | `unicode-width` + `vte` (ANSI parser) | Correct width for CJK/emoji/escape codes |
| Figlet | `figlet-rs` (embedded, no subprocess) | Avoid spawning `figlet` |
| File watching | `notify` v6 | Cross-platform fs events for `--edit`/`--watch` |
| Errors | `anyhow` (binary) + `thiserror` (library types) | Standard pattern |
| Logging | `tracing` + `tracing-subscriber` | Structured logs behind `--verbose` |

**Avoid:**
- `tokio` / async — pure CPU + sync `/proc` reads, async adds overhead with no benefit
- `sysinfo` as primary source — convenient but adds ~10ms overhead; prefer direct `/proc` reads, use `sysinfo` only as fallback for things that are genuinely hard to parse
- Spawning external commands in hot paths — every `Command::new()` costs 1–5ms

---

## 4. Architecture

### 4.1 Layer overview

```
┌────────────────────────────────────────────────────────────────┐
│  Config (TOML)                                                 │
│  └─ parsed into typed AST (themes, layout tree, widget params) │
└────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────────┐
│  Layer 1: Collectors                                           │
│  - Each collector is a `trait Collector` impl                  │
│  - Returns typed structs (CpuInfo, MemInfo, ...)               │
│  - Runs in parallel via rayon                                  │
│  - Cached in a `CollectorRegistry` for the run's lifetime      │
└────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────────┐
│  Layer 2: Widgets                                              │
│  - Each widget is a `trait Widget` impl                        │
│  - Takes widget params + access to CollectorRegistry           │
│  - Produces a `Cell` (Vec<StyledLine> + measured width)        │
└────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────────┐
│  Layer 3: Layout                                               │
│  - Composes cells into rows                                    │
│  - Handles alignment, gaps, padding, max-width                 │
│  - Produces a final `Frame` (Vec<StyledLine>)                  │
└────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────────┐
│  Layer 4: Renderer                                             │
│  - Writes Frame to stdout                                      │
│  - Honors --pipe (strip ANSI), TTY detection, terminal width   │
└────────────────────────────────────────────────────────────────┘
```

### 4.2 Core types

```rust
/// A styled segment: text with optional foreground/background/attributes.
pub struct Segment {
    pub text: String,
    pub style: Style,
}

/// A line is a vec of segments. Width is measured (ANSI-aware, unicode-aware).
pub struct StyledLine {
    pub segments: Vec<Segment>,
    pub width: usize,  // measured visual width
}

/// A cell is the output of a widget: lines + bounding box.
pub struct Cell {
    pub lines: Vec<StyledLine>,
    pub width: usize,   // max line width
    pub height: usize,  // lines.len()
}

/// Style: colors + attributes. Resolves theme refs at construction.
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
}

pub enum Color {
    Named(NamedColor),       // black, red, green, ...
    Rgb(u8, u8, u8),
    Hex(String),             // pre-validated
    Gradient(Vec<Color>),    // per-char interpolation, applied at render
}
```

### 4.3 Collectors

```rust
pub trait Collector: Send + Sync {
    /// Stable name for config refs (e.g. "cpu", "mem").
    fn name(&self) -> &'static str;

    /// Collect data. Returns serde_json::Value for uniform access.
    /// Using Value (not concrete types) lets widgets reference any field
    /// dynamically via `${data.cpu.usage}` without per-collector glue.
    fn collect(&self) -> Result<serde_json::Value, CollectorError>;
}
```

**Why `serde_json::Value`?** Widget config references fields by string path (`${data.cpu.usage}`). Using a uniform JSON-like value means:
- Adding a new collector requires zero changes to the expression evaluator
- Users can write `${data.cpu.cores}` or `${data.cpu.temp}` without us pre-declaring every field
- Internally each collector still works with its concrete struct, then `serde_json::to_value(&self.data)` at the boundary

> **Conscious tech debt (TODO post-v0.2):** `serde_json::Value` has extra allocs and gives up compile-time field checking. The clean long-term shape is: collectors return typed structs implementing a `trait IntoCollectorValue { fn as_value(&self) -> CollectorValue }`, with the conversion happening at the registry boundary. Skipping for v0.1 because the boilerplate cost is high and the perf cost is small (these structs are tiny). Revisit when a real second consumer of collector data exists (e.g. `--json` output, plugin system).

**Registry:**
```rust
pub struct CollectorRegistry {
    cache: HashMap<&'static str, Result<Value, CollectorError>>,
}

impl CollectorRegistry {
    /// Eagerly runs all referenced collectors in parallel.
    /// Called once at the start of a render pass.
    pub fn prime(&mut self, collectors: &[Box<dyn Collector>], referenced: &HashSet<&str>);

    /// Lookup with optional dotted path: "cpu.usage" → cache["cpu"]["usage"]
    pub fn get(&self, path: &str) -> Option<&Value>;
}
```

The `referenced` set comes from a pre-pass over the layout tree — we only collect what the config actually uses. Cuts cold-start time significantly for minimal configs.

### 4.4 Widgets

```rust
pub trait Widget: Send + Sync {
    fn render(
        &self,
        registry: &CollectorRegistry,
        theme: &Theme,
        max_width: Option<usize>,
    ) -> Result<Cell, RenderError>;
}
```

Widgets are constructed from config (`WidgetConfig` enum, one variant per widget type) and own their resolved parameters. They do not mutate the registry.

### 4.5 Layout

```rust
pub struct Layout {
    pub rows: Vec<Row>,
    pub gap: usize,           // vertical gap between rows
    pub align: HAlign,        // horizontal alignment of rows
}

pub struct Row {
    pub cells: Vec<Box<dyn Widget>>,
    pub gap: usize,           // horizontal gap between cells
    pub align: VAlign,        // vertical alignment within row (top/middle/bottom)
}

pub enum HAlign { Left, Center, Right }
pub enum VAlign { Top, Middle, Bottom }
```

**Render algorithm for a row:**
1. Render each cell → `Vec<Cell>`
2. Compute row height = `cells.iter().map(|c| c.height).max()`
3. Pad each cell's `lines` to `row_height` (insert empty lines per `VAlign`)
4. Pad each line in each cell to `cell.width` (right-pad with spaces, respecting trailing-style)
5. For row line index `i`: concatenate `cells[0].lines[i] ++ gap ++ cells[1].lines[i] ++ gap ++ ...`
6. Emit `row_height` final lines

For the top-level layout: render each row, separate by `layout.gap` blank lines, align horizontally if total row width < terminal width.

---

## 5. Project Structure

```
glamfetch/
├── Cargo.toml
├── README.md
├── CLAUDE.md                  # this file
├── LICENSE                    # MIT
├── src/
│   ├── main.rs               # CLI entry + dispatch
│   ├── cli.rs                # clap definitions
│   ├── config/
│   │   ├── mod.rs            # config loading + validation
│   │   ├── schema.rs         # serde structs for TOML
│   │   ├── expr.rs           # ${ns.path} evaluator with filter pipeline
│   │   └── filters.rs        # |humanize, |round, etc.
│   ├── theme/
│   │   ├── mod.rs
│   │   ├── palette.rs        # named palettes (catppuccin, gruvbox, nord)
│   │   └── gradient.rs       # per-char interpolation
│   ├── collect/
│   │   ├── mod.rs            # trait + registry
│   │   ├── system.rs         # os, kernel, hostname, user, uptime, shell
│   │   ├── cpu.rs
│   │   ├── memory.rs
│   │   ├── disk.rs
│   │   ├── gpu.rs
│   │   ├── battery.rs
│   │   ├── network.rs
│   │   ├── packages.rs
│   │   ├── desktop.rs        # DE/WM detection
│   │   └── datetime.rs
│   ├── widget/
│   │   ├── mod.rs            # trait + registry
│   │   ├── text.rs
│   │   ├── boxw.rs           # `box` is a reserved word
│   │   ├── bar.rs
│   │   ├── gauge.rs
│   │   ├── stack.rs
│   │   ├── row.rs
│   │   ├── spacer.rs
│   │   ├── separator.rs
│   │   ├── figlet.rs
│   │   └── ascii.rs
│   ├── layout/
│   │   ├── mod.rs
│   │   └── render.rs         # row composition algorithm
│   ├── render/
│   │   ├── mod.rs
│   │   ├── ansi.rs           # color → ANSI escape codes
│   │   ├── pipe.rs           # strip ANSI for --pipe
│   │   └── terminal.rs       # TTY detection, width, capabilities
│   ├── modes/
│   │   ├── once.rs           # default: render once and exit
│   │   ├── watch.rs          # --watch loop
│   │   └── edit.rs           # --edit split pane
│   └── error.rs
├── presets/
│   ├── default.toml
│   ├── catppuccin.toml
│   ├── gruvbox.toml
│   └── nord.toml
└── tests/
    ├── golden/               # snapshot tests of rendered output
    ├── collectors.rs
    └── layout.rs
```

---

## 6. Configuration

### 6.1 Location & resolution

- **Default:** `$XDG_CONFIG_HOME/glamfetch/config.toml`, falling back to `~/.config/glamfetch/config.toml`
- **Override:** `--config <path>`
- **First-run:** if no config exists, the binary embeds `presets/default.toml` and uses it in-memory. It does **not** auto-write the file (let the user opt in: `glamfetch --init` writes it).

### 6.2 Top-level schema

```toml
# Optional: load a preset as a base, then override below.
extends = "catppuccin"   # built-in preset name, or a path

[meta]
# Optional metadata, ignored by renderer.
name = "my config"

[theme]
# Variables anyone can reference via ${theme.name}
accent      = "#cba6f7"
muted       = "#6c7086"
fg          = "#cdd6f4"
bg          = "transparent"
border_fg   = "${theme.muted}"
bar_filled  = "${theme.accent}"
bar_empty   = "${theme.muted}"

[icons]
# Configurable icon palette; used by widgets that opt in.
# Glamfetch ships defaults for nerd-font and unicode-fallback sets.
set = "nerd-font"        # "nerd-font" | "unicode" | "ascii"

# Override individual icons:
[icons.overrides]
cpu  = "󰻠"
ram  = "󰍛"
disk = "󰋊"

[layout]
gap = 1                  # blank lines between rows
align = "left"           # left | center | right

# Each [[row]] is a horizontal strip.
# Each [[row.cell]] is a column within that row.
# Cells contain widgets.

[[row]]
align = "middle"         # vertical alignment for this row's cells

[[row.cell]]
widget = "figlet"
text   = "${data.system.hostname}"
font   = "slant"
color  = { gradient = ["${theme.accent}", "#f38ba8"] }

# Next row: 3 cells side-by-side
[[row]]
gap = 4                  # horizontal gap between cells

[[row.cell]]
widget = "box"
title  = "system"
title_color = "${theme.accent}"
border = "rounded"
padding = [0, 1]         # [v, h]
child = { widget = "text", content = """
  OS    ${data.os.name}
  Kern  ${data.kernel.version}
  Up    ${data.uptime|humanize}
  Shell ${data.system.shell}
""" }

[[row.cell]]
widget = "box"
title  = "hardware"
border = "rounded"
child = { widget = "stack", gap = 0, children = [
  { widget = "gauge", label = "CPU", value = "${data.cpu.usage}", max = 100, color = "${theme.bar_filled}", width = 20 },
  { widget = "gauge", label = "RAM", value = "${data.mem.percent}", max = 100, color = "${theme.bar_filled}", width = 20 },
  { widget = "gauge", label = "Disk", value = "${data.disk.percent}", max = 100, color = "${theme.bar_filled}", width = 20 },
] }

[[row.cell]]
widget = "box"
title  = "now"
border = "rounded"
show_if = "${data.battery.present}"   # conditional render
child = { widget = "text", content = """
  Time  ${data.datetime.time}
  Date  ${data.datetime.date}
  Batt  ${data.battery.percent}%
""" }
```

### 6.3 Expression syntax

**One syntax everywhere: `${namespace.path}`.**

| Namespace | Meaning | Example |
|---|---|---|
| `theme.*` | Theme variables defined under `[theme]` | `${theme.accent}` |
| `data.*` | Collector output | `${data.cpu.usage}` |
| `env.*` | Environment variables | `${env.USER}` |
| `icons.*` | Configured icons (after icon-set resolution) | `${icons.cpu}` |

The same syntax works in widget parameter slots (`value = "${data.cpu.usage}"`) and inside string templates (`content = "CPU: ${data.cpu.name}"`). The evaluator doesn't care about context — a `${...}` reference produces a value, period. Filters chain after with `|`: `${data.uptime|humanize}`.

Why one syntax: less magic, less documentation, fewer mistakes.

### 6.4 Filters

Filters transform values: `${data.uptime|humanize}`, `${data.cpu.freq|round(2)}`.

**v1 filter set:**
- `humanize` — `3661` (seconds) → `"1h 1m"`, also for bytes
- `round(n)` — round to n decimals
- `truncate(n)` — truncate to n chars, append ellipsis
- `upper`, `lower`, `title`
- `pad(n, char='\u0020')` — pad to n chars
- `default(val)` — replacement if missing/null

Filters chain: `${data.uptime|round(0)|pad(6)}`.

### 6.5 Conditional rendering

Any widget config can include `show_if = "${...}"`:
- If the resolved value is **falsy**, the widget renders as a zero-size empty cell (other cells in the row close up; row gap is *not* doubled)
- This makes configs portable across machines (e.g. battery widget on a desktop without battery)

**Truthy/falsy is JSON semantics, not JavaScript semantics:**

| Falsy | Truthy |
|---|---|
| `false` | `true` |
| `null` | any non-empty string, **including** `"0"`, `"false"`, `"no"` |
| `0`, `0.0` | any non-zero number |
| `""` (empty string) | any non-empty array or object |
| `[]` (empty array) | |
| `{}` (empty object) | |

This is intentional — JS truthiness rules (`"0"` is truthy) confuse people in template languages. JSON rules are simpler and more predictable. Explicit numeric/bool comparison filters (`|eq(0)`, `|gt(50)`) come post-MVP if there's demand.

### 6.6 Theme inheritance via `extends`

`extends = "catppuccin"` loads the preset, then the current file deep-merges on top. Resolution order:
1. Built-in preset name (matches a file in `presets/`)
2. Absolute path
3. Path relative to the current config file
4. Error

`extends` can be a string (single base) or an array (chain). **Array merge order: index 0 is the base, each subsequent entry overrides the previous, and the current file overrides them all** (same semantics as CSS cascade / Nix overlay composition). Deep merge: maps are merged recursively, arrays are *replaced* (not concatenated) — this avoids surprising "what order did my widgets end up in" bugs when extending presets.

---

## 7. Collectors (MVP)

Each collector returns a `serde_json::Value`-compatible struct. Fields below are the public surface.

### `system`
```rust
{ hostname: String, user: String, shell: String, terminal: String, locale: String }
```

### `os`
```rust
{ name: String, version: String, id: String, codename: Option<String> }
```
Source: `/etc/os-release`.

### `kernel`
```rust
{ name: String, version: String, arch: String }
```
Source: `uname()` via `rustix`.

### `uptime`
```rust
{ seconds: u64, pretty: String }
```
Source: `/proc/uptime`. `pretty` is pre-formatted (e.g. `"3 days, 2h 14m"`).

### `cpu`
```rust
{
  name: String,           // model name from /proc/cpuinfo
  cores: u32,             // physical
  threads: u32,           // logical
  usage: f32,             // % over a sample window — see note below
  freq_mhz: f32,          // average current
  temp_c: Option<f32>,    // hwmon, None if unavailable
}
```
**Usage measurement (perf-critical):** CPU usage requires *two* `/proc/stat` snapshots with a delay between them. Strategy:
- **One-shot mode:** read `/proc/stat`, sleep **50ms** (not 100ms — accept slightly noisier reading), read again, compute delta. Document this clearly: `${data.cpu.usage}` costs ~50ms once per run; if your config doesn't reference it, no cost is paid. The collector pre-pass ensures unreferenced CPU usage is never measured.
- **`--watch` mode:** the watch loop keeps the previous tick's `/proc/stat` snapshot in memory and computes delta against it. No sleep needed after the first tick. First tick still pays the 50ms cost.
- **Reference `${data.cpu.freq}` or `${data.cpu.temp}` only:** entire CPU collector skips the dual-read and runs in <1ms.

The 50ms is the dominant cost in any config that shows live CPU usage. Setting a `<30ms typical run` target *and* showing `${data.cpu.usage}` is incompatible — the default preset must not reference `usage` if we want the perf budget, OR we accept a `<80ms` budget for configs that opt in to live usage. Default preset choice: **show usage** (it's what users expect) and document the cost honestly.

**Temp:** scan `/sys/class/hwmon/*/name` for `coretemp` or `k10temp`, read `temp1_input` (millidegrees).

### `memory`
```rust
{
  total_bytes: u64,
  used_bytes: u64,
  free_bytes: u64,
  available_bytes: u64,
  percent: f32,
  swap_total: u64,
  swap_used: u64,
}
```
Source: `/proc/meminfo`. `used` = `total - available` (the standard Linux semantic).

### `disk`
```rust
// Aggregated view across all configured mounts:
{ total_bytes: u64, used_bytes: u64, free_bytes: u64, percent: f32,
  mounts: [{ path: String, total: u64, used: u64, free: u64, percent: f32, fs: String }] }
```
Source: `/proc/mounts` for the list, `statvfs()` for stats.
Default filter: skip pseudo-filesystems (`tmpfs`, `devtmpfs`, `proc`, `sys`, `cgroup*`, `overlay`, etc.) and bind mounts.

### `gpu`
```rust
{ present: bool, primary: Option<{ vendor: String, model: String, driver: Option<String> }>,
  all: [{ vendor, model, driver }] }
```
**v1 strategy:** read `/sys/class/drm/card*/device/{vendor,device}`, map PCI IDs to vendor names via an embedded minimal lookup table (NVIDIA = `0x10de`, AMD = `0x1002`, Intel = `0x8086`). Model name: try `/sys/class/drm/card*/device/uevent` and `lspci -mm -nn -k` as a *last resort* (subprocess, only if available and `gpu.detailed = true` in config).

If detection fails or no GPU: `present = false`, primary is `None`, all is empty. Widgets referencing `gpu.*` show `--`.

### `battery`
```rust
{ present: bool, percent: Option<u8>, status: Option<String>, time_remaining_min: Option<u32> }
```
Source: `/sys/class/power_supply/BAT*/`. `present = false` if no BAT directory.

### `network`
```rust
{
  interfaces: [{ name, ip4: Option<String>, ip6: Option<String>, mac: String, up: bool }],
  primary: Option<{ name, ip4, ssid: Option<String> }>,
  ssid: Option<String>,   // shorthand for primary.ssid
}
```
Source: `/sys/class/net/*` + `/proc/net/route` for default route. SSID via `iwgetid -r` if available (subprocess — acceptable here, it's typically <2ms). `primary` is the interface owning the default route.

### `packages`
```rust
{ total: u32, by_manager: { pacman: u32, flatpak: u32, snap: u32, apt: u32, ... } }
```
**Strategy:** counts via direct filesystem reads where possible:
- pacman: count files in `/var/lib/pacman/local/` (excluding `ALPM_DB_VERSION`)
- flatpak: count subdirs in `/var/lib/flatpak/app/` + `~/.local/share/flatpak/app/`
- apt: count `Package:` lines in `/var/lib/dpkg/status` (mmap + memchr for speed)
- snap: count subdirs in `/snap/`

Only run counters whose path exists (skip silently otherwise — no errors).

### `desktop`
```rust
{ de: Option<String>, wm: Option<String>, session_type: Option<String> }
```
Source: env vars (`XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY`, `DISPLAY`). Minimal heuristics for v1.

### `datetime`
```rust
{ time: String, date: String, iso: String, weekday: String, timestamp: i64 }
```
Source: `chrono` crate. Formats configurable per-widget via filters.

---

## 8. Widgets (MVP)

All widgets share these optional fields:
- `show_if` (expr) — conditional render
- `padding` — `[v, h]` or `[top, right, bottom, left]`
- `margin` — same shape as padding

### `text`
```toml
widget = "text"
content = "..."          # template string with {refs} and \n
color = "${theme.fg}"    # default theme fg
align = "left"           # left | center | right (multi-line)
wrap = false             # if true, hard-wrap at max_width
```
- `content` is evaluated as a template. Newlines preserved.
- Leading indentation on each content line is trimmed *consistently* (like Rust's `trim_start_matches` on the smallest common indent) — makes multi-line strings in TOML readable.

### `box`
```toml
widget = "box"
title = "system"         # optional, rendered in top border
title_color = "${theme.accent}"
title_align = "left"     # left | center | right
border = "rounded"       # rounded | sharp | double | thick | ascii | none
border_color = "${theme.border_fg}"
padding = [0, 1]
child = { ... }          # nested widget config
```
Box characters per border style (Unicode):
- `rounded`: `╭ ╮ ╰ ╯ ─ │`
- `sharp`:   `┌ ┐ └ ┘ ─ │`
- `double`:  `╔ ╗ ╚ ╝ ═ ║`
- `thick`:   `┏ ┓ ┗ ┛ ━ ┃`
- `ascii`:   `+ + + + - |`

### `bar`
```toml
widget = "bar"
value = "${data.cpu.usage}"
max = 100
width = 20               # in chars
filled_char = "█"
empty_char = "░"
color = "${theme.bar_filled}"  # or { gradient = [...] } — interpolates along the bar
empty_color = "${theme.bar_empty}"
```

### `gauge`
A composite widget: `[label] [value]% [bar]`. Convenience over manual composition.
```toml
widget = "gauge"
label = "CPU"
value = "${data.cpu.usage}"
max = 100
width = 20               # of the bar portion only
show_percent = true
color = "${theme.bar_filled}"
```

### `stack`
Vertical stack of children.
```toml
widget = "stack"
gap = 0                  # blank lines between children
align = "left"           # horizontal alignment of children within stack width
children = [ {...}, {...}, ... ]
```

### `row` (inner row)
Horizontal row *within a cell* — for sub-row composition.
```toml
widget = "row"
gap = 2
align = "middle"         # vertical alignment
children = [ {...}, {...}, ... ]
```

### `spacer`
```toml
widget = "spacer"
width = 4                # for horizontal contexts
height = 1               # for vertical contexts
```

### `separator`
```toml
widget = "separator"
char = "─"
length = 20              # or "auto" → fills parent width
color = "${theme.muted}"
```

### `figlet`
```toml
widget = "figlet"
text = "${data.system.hostname}"
font = "slant"           # built-in fonts: slant, standard, big, small, mini
color = "${theme.accent}"      # supports gradient
```
Bundle ~6 figlet fonts as embedded `&[u8]` (selected for size/quality). Users referencing an unknown font name fall back to `standard` with a warning.

### `ascii`
```toml
widget = "ascii"
source = "inline"        # "inline" | "file"
content = """
  /\\_/\\
 ( o.o )
  > ^ <
"""
# or:
# source = "file"
# path = "~/.config/glamfetch/cat.txt"
color = "${theme.accent}"
```

---

## 9. Layout System

### 9.1 Width handling

**The width contract (read carefully):**

`Widget::render` takes `max_width: Option<usize>`. The contract is:
- `None` — "claim whatever width you need". Used at the top level for the first render of each cell.
- `Some(w)` — "the parent has reserved exactly `w` columns for you; do not exceed". Used when a parent widget *knows* its inner width and passes it down.

**Which widgets propagate width to children:**
- `box` knows its inner width *only after* the child is rendered (chicken/egg). Resolution: box calls child with `max_width: None` first, measures the child's actual width, then constructs the border around it. The box's *outer* width = child width + 2*border + 2*padding.
- `stack` passes `max_width` through unchanged: a stack inside a box that has a known inner width passes that down to each child.
- `row` (inner) computes per-child widths only if all children declare fixed widths; otherwise passes `None` to each.
- `separator` with `length = "auto"` requires a non-`None` `max_width` from its parent. If parent passes `None`, separator falls back to its `default_length` (config-set, defaults to 20).

**Top-level overflow:**
- After all cells render, layout knows actual row widths.
- If total row width > terminal width: **truncate trailing cells, do not wrap** for v1.
- Truncation emits a warning at `--verbose`.
- Future: a `flex` mode where cells declare `min_width` / `max_width` / `flex` weights. Not in v1.

### 9.2 Padding propagation

Padding is applied **outside** the cell's rendered content. A `box` widget with `padding = [0, 1]` adds 1 char of space inside the border on each side. A top-level cell with `padding` adds outer whitespace.

### 9.3 Empty cells

A cell whose widget evaluates `show_if = false` becomes a zero-width zero-height cell. The row's other cells close up; gap between them is *not* doubled.

---

## 10. CLI

```
glamfetch [OPTIONS]

OPTIONS:
  -c, --config <PATH>       Path to config file (overrides default)
      --pipe                Plain output: strip ANSI, ASCII border fallback, no figlet
      --json                Emit all collected data as JSON to stdout (no rendering)
      --watch [INTERVAL]    Re-render on an interval (default: 1s)
      --edit                Open the live preview pane (pair with tmux/zellij for split)
      --init                Write the default preset to the config path and exit
      --list-presets        List built-in presets and exit
      --print-config        Print the resolved config (after extends/merging) and exit
      --print-data          Print collected data as a human-readable summary
  -v, --verbose             Enable debug logging to stderr
  -V, --version             Print version and exit
  -h, --help                Print help

ENVIRONMENT:
  GLAMFETCH_CONFIG          Override default config path (lower priority than --config)
  NO_COLOR                  Disable ANSI colors (standard env)
  GLAMFETCH_LOG             Log filter (tracing-subscriber syntax)
```

### `--pipe` semantics

When `--pipe` is set or stdout is not a TTY:
- All ANSI escape codes stripped (use the renderer's pipe mode)
- Box borders fall back to `ascii` style if currently set to a Unicode style
- Figlet still works (it's just ASCII art)
- Gradients collapse to the first color (which is then also stripped) — effectively plain text
- Output is suitable for piping to `column`, `grep`, etc.

### `--json` semantics

When `--json` is set, the binary:
- **Skips config loading entirely** (no read, no `extends` resolution, no `[icons]` merging). Rationale: `--json` is for raw collector data; theme/layout/icons are render-layer concerns. Skipping config read is faster and means `--json` works even if your config has a parse error (useful for debugging).
- Runs every collector unconditionally (no pre-pass, since there's no layout to scan)
- Emits a single JSON object: `{ "system": {...}, "cpu": {...}, "mem": {...}, ... }`
- No ANSI, no figlet, no rendering
- Exits 0 on success even if individual collectors fail (failures become `null` for that collector with a `_errors` map at the top level: `{ "_errors": { "gpu": "no /sys/class/drm entries" }, ... }`)

Use cases: scripting (`glamfetch --json | jq .cpu.usage`), debugging which fields are available for `${data.*}` references, integration with status bars (waybar custom modules).

### `--watch` semantics

- Clears screen and re-renders every interval
- Collectors with mutable state (CPU usage) maintain previous-tick snapshots, so subsequent ticks measure deltas without sleeping
- Ctrl+C exits cleanly (restore cursor, clear screen if entered alt-screen)

### `--edit` semantics

- Splits terminal: left half = config file in `$EDITOR`, right half = live preview
- Watches config file via `notify`; re-parses + re-renders on save
- If parse fails: preview shows the error inline, doesn't crash
- Implementation: spawn `$EDITOR` in a subprocess in left pane (tmux-style pane management is overkill — instead, *recommend* the user run it inside `tmux`/`zellij` and provide a `glamfetch --preview` mode that's just the right pane)

> **Implementation note:** Building real split-pane terminal multiplexing is a rabbit hole. **For v1, ship `--edit` as just the live preview pane** (continuously re-renders the config file on change). Document that users can pair it with `tmux`/`zellij`/`wezterm` for the split layout. This is honest about the boundary and avoids reinventing tmux.

---

## 11. Performance Targets

These are **post-measurement targets**. Do not commit to numbers in code or docs until Phase 1 produces a baseline measurement on Michael's i7-13700K. The numbers below are *aspirational starting points* — tighten or loosen after the first end-to-end run.

| Metric | Aspirational | Hard ceiling |
|---|---|---|
| Cold start, empty config (`glamfetch --version`) | "instant" — measure first | 15ms |
| Default preset, no `${data.cpu.usage}` reference | < 30ms | 80ms |
| Default preset *with* live CPU usage | < 80ms (50ms is the sample window) | 150ms |
| Heaviest config (all collectors, all widgets, live usage) | < 100ms | 200ms |
| `--watch` per-tick overhead (steady state) | < 10ms | 30ms |
| Binary size (release, stripped) | < 4 MB | 8 MB |

**The CPU usage trade-off is real and unavoidable.** Sampling CPU usage requires two `/proc/stat` reads with a delay; the delay sets the floor. There is no clever workaround for one-shot mode — either you sample and pay the cost, or you skip and show stale data. The pre-pass that skips unreferenced collectors is the main mitigation.

**Provide a built-in `--time` flag** (or `GLAMFETCH_LOG=glamfetch=debug`) that breaks down per-collector and per-widget timings. This is more valuable than enforcing a number in CI.

**Optimisation principles (apply only after measurement shows a miss):**
- Skip collectors not referenced anywhere in the layout (pre-pass) — non-negotiable, always on
- Run collectors in parallel with `rayon::scope`
- Mmap large files (`/var/lib/dpkg/status`) instead of reading
- Pre-allocate strings with `String::with_capacity` where size is roughly known
- Avoid `format!` in hot loops (use `write!` to a pre-sized `String`)
- LTO + `codegen-units = 1` + `panic = "abort"` in release profile

---

## 12. Error Handling

**Policy:** *show the user something useful even when things fail.*

| Failure mode | Behavior |
|---|---|
| Collector fails entirely (e.g. no GPU) | Widget referencing it shows `--` placeholder (configurable globally as `theme.missing_placeholder`) |
| Collector field missing (e.g. `${data.cpu.nonexistent}`) | Shows `--` placeholder + warning at `--verbose` |
| Widget construction fails (bad config) | Hard error at startup with file:line if possible |
| Render error (e.g. figlet OOM) | That cell shows `[render error]` in `${theme.danger}` color (default red); rest of layout renders |
| Config parse error | Hard error to stderr with a snippet showing the offending region |
| Theme variable undefined (`${unknown}`) | Hard error at startup |
| `extends` target not found | Hard error at startup |

In `--watch`/`--edit`: parse errors don't crash; they replace the layout with an error pane until the file is fixed.

---

## 13. Terminal & Platform Compatibility

### Platform scope (explicit)

**v1 is Linux-only by design**, but the codebase is structured so that *only collectors are platform-bound*:
- `src/collect/*` — all Linux-specific (`/proc`, `/sys`, `statvfs`, hwmon)
- `src/render/`, `src/widget/`, `src/layout/`, `src/theme/`, `src/config/` — **portable** (no `/proc`, no syscalls, only stdlib + `crossterm`)

When adding macOS/BSD/Windows post-MVP, the collector trait stays; only the implementations behind it change. Collectors gain `#[cfg(target_os = "linux")]` gates and parallel `macos`/`bsd` modules. Tests in `tests/collectors.rs` are similarly gated.

**Windows is a separate concern.** `crossterm` handles ConPTY; that's the rendering side. The collector side would need a full `sysinfo`/WMI port. Not in scope for the foreseeable future — note as out of scope, not "soon".

### Terminal compatibility

**Targeted terminals (must look correct):**
- kitty, alacritty, foot, wezterm, ghostty, konsole, gnome-terminal, xterm

**Capability detection (best-effort):**
- TTY detection via `crossterm::tty::IsTty`
- True color: assume yes if `COLORTERM` ∈ {`truecolor`, `24bit`}, otherwise downgrade gradient/hex colors to nearest 256-color
- Unicode width: trust `unicode-width` crate; no double-checking against terminal
- Nerd Font detection: **not possible reliably** — provide `icons.set = "nerd-font" | "unicode" | "ascii"` and let the user choose. Default to `nerd-font` (matches the audience).

`NO_COLOR=1` honored: disables all coloring.

---

## 14. Implementation Phases & Release Strategy

> **Release strategy:** The spec describes the full v1 *vision*. We ship that vision in two releases:
> - **v0.1.0** — walking skeleton, intentionally minimal. Goal: validate the architecture, get the binary into someone else's hands, build confidence in the codebase. Not the marketing launch.
> - **v0.2.0** — the real launch with the differentiators (`--edit`, gradients, full widget set, all presets). This is the version that goes on r/unixporn.
>
> The two-stage release prevents architecture refactors during the launch push: if v0.1.0 catches structural problems, fix them before adding feature surface.

### Phase 0 — Scaffolding
- Cargo project, CI (GitHub Actions: fmt + clippy + test), MIT license
- `error.rs` with top-level error types
- `cli.rs` skeleton parsing `--config`, `--print-config`, `--version`
- Basic config loading (load TOML, validate, no widgets yet)
- `--init` writes the embedded `presets/default.toml` placeholder

### Phase 1 — Walking skeleton
- Implement 3 collectors: `system`, `os`, `kernel` (simplest)
- Implement 2 widgets: `text`, `stack`
- Implement layout primitive: rows of cells, no styling, no padding
- End-to-end: load config → collect → render → print to stdout
- Snapshot tests for the rendered output of a fixed config

**Definition of done:** `glamfetch` with a hand-written config prints "hostname: foo / os: bar / kernel: baz" laid out in a 2-column row.

### Phase 2 — Styling foundation
- Theme system: variable substitution, palette loading from preset name
- Unified `${...}` expression evaluator with namespaces (`theme`, `data`, `env`, `icons`)
- Filter pipeline (`humanize`, `round`, `truncate`, `upper`, `lower`, `pad`, `default`)
- ANSI renderer with truecolor + 256-color fallback
- `Style` propagation through cells/lines
- `--pipe` mode (strip ANSI, ASCII border fallback)
- Snapshot tests with styled output

### Phase 3 — v0.1.0 ship scope
- Two more collectors: `cpu`, `memory` (the user-visible essentials)
- Two more widgets: `box` (rounded border only), `gauge`
- `--json` output mode
- `--print-config`, `--print-data`
- Write `presets/default.toml`
- `show_if` evaluation (with documented JSON-truthy semantics)
- README with one screenshot, install instructions

**🚢 Release v0.1.0 here.** Tag, push tarball, share with a few friends to bang on. *Do not* post to r/unixporn yet — feature set is too thin.

**Definition of done for v0.1.0:**
- [ ] All Phase 0–3 items checked
- [ ] `cargo install --path .` works on a fresh EndeavourOS install
- [ ] Default preset renders correctly on Michael's machine
- [ ] `--json` produces valid JSON consumable by `jq`
- [ ] `--pipe` produces zero-ANSI output (regex-verified)
- [ ] Snapshot tests pass for every widget + every border style
- [ ] Perf budget measured and documented in README (not aspirational anymore)

---

### Phase 4 — Remaining collectors
- `uptime`, `disk`, `gpu`, `battery`, `network`, `packages`, `desktop`, `datetime`
- Per-collector unit tests with fixture `/proc` data where possible
- Parallel collection via rayon
- Collector pre-pass: collect only what's referenced

### Phase 5 — Remaining widgets
- All border styles (sharp, double, thick, ascii)
- `bar` (standalone, separate from `gauge`)
- `row` (inner), `spacer`, `separator`
- `figlet` with embedded fonts
- `ascii`

### Phase 6 — Differentiators
- Gradient implementation (per-char interpolation)
- `presets/catppuccin.toml`, `presets/gruvbox.toml`, `presets/nord.toml`
- `--list-presets`
- `--watch` mode with alt-screen handling and clean exit
- `--edit` (preview-pane mode — re-render on file save, pair with tmux/zellij)

### Phase 7 — v0.2.0 launch prep
- README with at least 3 distinct rices (screenshots / `asciinema` GIFs)
- Man page (via `clap_mangen`)
- Shell completions (via `clap_complete`)
- AUR PKGBUILD + standalone Linux release tarball
- Tag v0.2.0

**🚀 Launch v0.2.0.** Post to r/unixporn, r/rust, Hacker News. Lead the launch post with `--edit` (live preview is the single biggest differentiator).

---

## 15. Out of Scope (post-MVP)

Track these as GitHub issues but do not implement in v1:

- **Plugin system** (Lua via `mlua`, or dynamic library loading)
- **Image rendering** (kitty graphics protocol, sixel, chafa adapter)
- **History/sparkline widgets** (require persistent state between runs)
- **Chart widgets** (line, bar chart over historical data)
- **macOS / BSD / Windows** support (requires collector abstraction layer)
- **Animated transitions** in `--watch` mode
- **Web/HTML output mode** (render config to HTML — for sharing rices)
- **Per-character animations** (typewriter, glitch effects)
- **Network speed / per-interface traffic widgets** (requires sampling)

---

## 16. Testing Strategy

### Unit tests
- Per collector: feed in known `/proc` fixture data, assert parsed struct fields
- Expression evaluator: parse `${theme.accent}`, `${data.cpu.usage}`, `${env.HOME}`, with filters `${data.uptime|humanize|truncate(20)}`
- Filter pipeline: each filter with edge cases

### Snapshot tests (golden)
Use `insta` crate. Each snapshot test:
1. Loads a fixed config from `tests/configs/`
2. Renders with a *mocked* collector registry (deterministic data)
3. Compares output bytes to `tests/golden/<name>.txt`

Snapshots cover, **by release**:

- **v0.1.0** (Phases 1–3): each widget standalone (`text`, `stack`, `box` rounded, `gauge`), gauge fill levels at 0/50/100, multi-cell rows, padded boxes, `show_if = false`, `--pipe` output (no ANSI codes), `--json` shape stability.
- **v0.2.0** (Phases 4–6): remaining border styles (sharp/double/thick/ascii), `bar`, `figlet` per font, `ascii` widget, gradient rendering (per-char interpolation across a known palette), all 4 presets rendered against a fixed mocked data fixture.

### Integration tests
- Run the binary against a real system, assert it doesn't error and produces non-empty output
- `--init` then `glamfetch` round-trip
- `--pipe` output contains no ANSI codes (regex check)

### Manual test matrix (release checklist)
- Run on Michael's EndeavourOS desktop (KDE Wayland, RTX 4070, true color)
- Run on a fresh Ubuntu container (no GPU, minimal packages)
- Run on a laptop (battery present)
- Run inside `tmux` and outside
- Run with `NO_COLOR=1`
- Run with stdout redirected to a file

---

## 17. Open Questions / Decisions to Defer

These are explicitly *not* blocking v1 — flag in code with `TODO(post-mvp)`:

1. **Caching:** should we cache slow collectors (GPU, packages) on disk between runs? Fastfetch does. Defer until measured: only cache if a collector exceeds 20ms.
2. **Theme switching at runtime:** in `--watch`, should config reload trigger smooth transition or just snap? Snap for v1.
3. **Markdown in text content:** support `**bold**` / `*italic*` in `content` strings? Tempting but adds complexity. Defer — users can apply `bold = true` at the widget level.
4. **Localisation:** `humanize` output is English (`"1 day, 2h"`). Locale-aware? Defer.
5. **Number i18n (decimal separator):** percentages and counts use `.` decimal. For European users `,` is conventional in display. Probably honor `LC_NUMERIC` via `num-format`, but defer — TOML can also let users override via filters. Note it, don't solve it in v1.
6. **Distro logos:** ship per-distro ASCII art as a `distro` widget? Probably yes post-MVP — it's a popular feature — but it's a different scope (sourcing/licensing logos). Defer.

---

## 18. Naming, Repo, Conventions

- **Crate name:** `glamfetch`
- **Binary name:** `glamfetch`
- **Repo:** `synnode/glamfetch` (or wherever Michael wants it hosted)
- **License:** MIT
- **MSRV:** Rust 1.75+ (no nightly features)
- **Code style:** rustfmt default, clippy `pedantic` opt-in module by module (don't blanket-enable)
- **Commit style:** Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`)
- **Documentation:** all rustdoc comments and code comments in English (Michael's standing rule)

---

## 19. Done = ?

The "definition of done" lives inside each phase (see §14). To summarise:

**v0.1.0 ships when** all Phase 0–3 items in §14 are checked. This is the *internal* milestone — small audience, gathering feedback.

**v0.2.0 ships when:**
- [ ] All Phase 0–7 items checked
- [ ] All listed collectors return correct data on Michael's machine
- [ ] All listed widgets render correctly in snapshot tests
- [ ] `--pipe` produces clean plain output
- [ ] `--watch` doesn't leak resources over 1 hour of running
- [ ] `--edit` re-renders within 50ms of a config save
- [ ] All 4 presets render without errors
- [ ] README has at least 3 screenshots of distinct rices
- [ ] Performance numbers measured and documented (no aspirational claims)
- [ ] Binary published as GitHub release with Linux x86_64 tarball
- [ ] AUR package submitted

After v0.2.0: post a "show HN" / `r/unixporn` / `r/rust` thread with a screenshot. The live preview tool (`--edit`) is the single biggest differentiator — lead with that in the launch post.