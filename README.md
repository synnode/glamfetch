# glamfetch

![CI](https://github.com/synnode/glamfetch/actions/workflows/ci.yml/badge.svg)

A glamorous system-info fetch tool for the terminal. Alternative to
`fastfetch` / `neofetch` for rice enthusiasts who want full styling control
without sacrificing speed.

> **Status:** post-v0.1.0, Phase 4 complete on `master`. See
> [`.docs/glamfetch-spec.md`](.docs/glamfetch-spec.md) for the full v1 spec
> and phase roadmap. The v0.2.0 launch is the version that goes on r/unixporn.

```
╭ system ─────────────────╮  ╭ hardware ────────────────────╮
│ host    michael-desktop │  │ CPU    3% ░░░░░░░░░░░░░░░░░░ │
│ user    michael         │  │ RAM   21% ████░░░░░░░░░░░░░░ │
│ os      EndeavourOS     │  ╰──────────────────────────────╯
│ kernel  6.19.6-arch1-1  │
│ shell   /usr/bin/zsh    │
╰─────────────────────────╯
```

## Install

From source:
```sh
git clone https://github.com/synnode/glamfetch
cd glamfetch
cargo install --path .
```
Requires Rust 1.85+.

## Run

```sh
glamfetch                  # render default preset
glamfetch --pipe           # plain output, no ANSI
glamfetch --json           # all collector data as JSON for jq / scripting
glamfetch --print-data     # human-readable dump of available ${data.*} refs
glamfetch --print-config   # resolved config
glamfetch --init           # write default preset to ~/.config/glamfetch/config.toml
glamfetch --list-presets
```

## Configure

Default config path (XDG): `~/.config/glamfetch/config.toml`.

```toml
[theme]
accent     = "#cba6f7"
muted      = "#6c7086"
bar_filled = "${theme.accent}"

[[row]]
gap = 2

[[row.cell]]
widget = "box"
title  = "system"
title_color = "${theme.accent}"
border = "rounded"
padding = [0, 1]
child = { widget = "text", content = """
  host    ${data.system.hostname}
  os      ${data.os.name}
  kernel  ${data.kernel.version}
""" }
```

See [`.docs/glamfetch-spec.md`](.docs/glamfetch-spec.md) §6 for the full
schema, §7 for collector field listings, §8 for widget reference.

## Scope

What's in (master, post-v0.1.0):
- Collectors: `system`, `os`, `kernel`, `uptime`, `cpu`, `mem`, `disk`,
  `gpu`, `battery`, `network`, `packages`, `desktop`, `datetime`
- Widgets: `text`, `stack`, `box` (rounded border), `gauge`
- Themes with `${theme.*}` variable resolution
- Filters: `humanize`, `round`, `truncate`, `upper`/`lower`/`title`, `pad`, `default`
- `show_if` conditional rendering (JSON-truthy semantics)
- True-color ANSI + 256-color quantizer fallback
- `--pipe`, `--json`, `--print-data`, `NO_COLOR`, non-TTY auto-detect
- Parallel collector execution via `rayon`
- Collector pre-pass: only collectors referenced by the layout actually run

Coming in v0.2.0:
- `--watch` + `--edit` (live preview)
- Remaining widgets (bar, separator, spacer, figlet, ascii, inner row)
- Gradients (per-char interpolation)
- Catppuccin / Gruvbox / Nord presets
- All box border styles (sharp, double, thick, ascii)

## Performance

Measured on an Intel i7-13700K, release build (LTO + strip), Linux:

| Command | Time |
|---|---|
| `glamfetch --version` (cold start) | **2-4ms** |
| `glamfetch --pipe`, default preset (CPU + RAM gauges) | **59-60ms** |
| `glamfetch --pipe`, minimal preset (no CPU usage) | **6-7ms** |
| `glamfetch --json` (runs every collector) | **59-60ms** |
| Binary size (release, stripped) | **2.1 MB** |

Two things are doing the heavy lifting here:

- **Collector pre-pass.** Before priming the registry the binary scans the
  config for `${data.<root>}` references and only runs collectors actually
  named. A preset that doesn't reference `${data.cpu.usage}` skips the 50ms
  sample window automatically.
- **Parallel collection.** Referenced collectors run on a `rayon` thread
  pool, so the heaviest config still bottlenecks on its single slowest
  collector (the CPU sample) instead of summing them.

## License

MIT. See [`LICENSE`](LICENSE).
