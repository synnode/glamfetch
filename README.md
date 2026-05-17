# glamfetch

A glamorous system-info fetch tool for the terminal. Alternative to
`fastfetch` / `neofetch` for rice enthusiasts who want full styling control
without sacrificing speed.

> **Status:** v0.1.0 — walking-skeleton release. See
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

## v0.1.0 scope

What's in:
- Collectors: `system`, `os`, `kernel`, `cpu`, `mem`
- Widgets: `text`, `stack`, `box` (rounded border), `gauge`
- Themes with `${theme.*}` variable resolution
- Filters: `humanize`, `round`, `truncate`, `upper`/`lower`/`title`, `pad`, `default`
- `show_if` conditional rendering (JSON-truthy semantics)
- True-color ANSI + 256-color quantizer fallback
- `--pipe`, `--json`, `--print-data`, `NO_COLOR`, non-TTY auto-detect

Coming in v0.2.0:
- `--watch` + `--edit` (live preview)
- Remaining collectors (uptime, disk, gpu, battery, network, packages, desktop, datetime)
- Remaining widgets (bar, separator, spacer, figlet, ascii, inner row)
- Gradients (per-char interpolation)
- Catppuccin / Gruvbox / Nord presets
- All box border styles (sharp, double, thick, ascii)

## Performance

Measured on an Intel i7-13700K, release build (LTO + strip), Linux:

| Command | Time |
|---|---|
| `glamfetch --version` (cold start) | **3-4ms** |
| `glamfetch --pipe` (default preset, includes CPU usage) | **53-57ms** |
| `glamfetch --json` (every collector) | **56-57ms** |
| Binary size (release, stripped) | **1.9 MB** |

The 50ms floor on default-preset render comes from CPU usage sampling: it
requires two `/proc/stat` snapshots with a delay between them (spec §11).
Configs that don't reference `${data.cpu.usage}` will render in ~5-7ms. The
collector pre-pass (Phase 4) will skip unreferenced collectors automatically.

## License

MIT. See [`LICENSE`](LICENSE).
