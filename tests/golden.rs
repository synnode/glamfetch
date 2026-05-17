//! Golden snapshot tests for the render pipeline.
//!
//! Each test loads a TOML config from `tests/fixtures/` (or inline), injects
//! deterministic collector data into a fresh [`CollectorRegistry`], renders
//! the styled frame, then writes it through the desired renderer mode for
//! snapshotting.

use glamfetch::collect::CollectorRegistry;
use glamfetch::config::ConfigFile;
use glamfetch::config::expr::StaticContext;
use glamfetch::layout::{Layout, Row};
use glamfetch::render::ansi;
use glamfetch::render::terminal::ColorMode;
use glamfetch::style::StyledLine;
use glamfetch::theme;
use serde_json::json;

fn parse(toml_text: &str) -> ConfigFile {
    toml::from_str(toml_text).expect("fixture config parses")
}

fn build_frame(cfg: ConfigFile, registry: &CollectorRegistry) -> Vec<StyledLine> {
    let resolved_theme = theme::resolve(&cfg.theme).expect("theme resolves");
    let ctx = StaticContext {
        theme: resolved_theme,
        icons: cfg.icons.overrides.clone(),
        env_allowed: false,
    };

    let rows: Vec<Row> = cfg
        .rows
        .into_iter()
        .map(|r| Row::build(r, &ctx))
        .collect::<Result<_, _>>()
        .expect("widget tree builds");

    let layout = Layout::new(rows, cfg.layout.gap);
    layout.render(registry).expect("render succeeds")
}

fn render_mode(frame: &[StyledLine], mode: ColorMode) -> String {
    let mut buf = Vec::new();
    ansi::write_frame(&mut buf, frame, mode).expect("write_frame");
    String::from_utf8(buf).expect("utf8 output")
}

fn mock_registry() -> CollectorRegistry {
    let mut reg = CollectorRegistry::new();
    reg.insert(
        "system",
        json!({
            "hostname": "snowbox",
            "user": "michael",
            "shell": "/usr/bin/zsh",
            "terminal": "xterm-kitty",
            "locale": "en_US.UTF-8",
        }),
    );
    reg.insert(
        "os",
        json!({
            "name": "EndeavourOS",
            "version": "2026",
            "id": "endeavouros",
            "codename": "mercury",
        }),
    );
    reg.insert(
        "kernel",
        json!({
            "name": "Linux",
            "version": "6.19.6-arch1-1",
            "arch": "x86_64",
        }),
    );
    reg
}

#[test]
fn two_cell_row_pipe() {
    let toml_text = include_str!("fixtures/two_cell_row.toml");
    let frame = build_frame(parse(toml_text), &mock_registry());
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn missing_field_falls_back_to_placeholder() {
    let toml_text = r#"
[layout]
gap = 0

[[row]]
gap = 0

[[row.cell]]
widget = "text"
content = "v=${data.system.does_not_exist|default(--)}"
"#;
    let frame = build_frame(parse(toml_text), &mock_registry());
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn stack_widget_vertical_gap() {
    let toml_text = r#"
[layout]
gap = 0

[[row]]
gap = 0

[[row.cell]]
widget = "stack"
gap = 1
children = [
  { widget = "text", content = "first" },
  { widget = "text", content = "second" },
  { widget = "text", content = "third" },
]
"#;
    let frame = build_frame(parse(toml_text), &mock_registry());
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn styled_text_truecolor() {
    let toml_text = r##"
[layout]
gap = 0

[theme]
accent = "#ff8800"

[[row]]
gap = 0

[[row.cell]]
widget = "text"
content = "host ${data.system.hostname}"
color = "${theme.accent}"
bold = true
"##;
    let frame = build_frame(parse(toml_text), &mock_registry());
    insta::assert_snapshot!(
        "styled_text_truecolor",
        render_mode(&frame, ColorMode::Truecolor)
    );
    insta::assert_snapshot!(
        "styled_text_palette256",
        render_mode(&frame, ColorMode::Palette256)
    );
    insta::assert_snapshot!("styled_text_none", render_mode(&frame, ColorMode::None));
}

#[test]
fn box_widget_rounded_with_title() {
    let toml_text = r##"
[layout]
gap = 0

[theme]
accent = "#ff8800"
muted  = "#666666"

[[row]]
gap = 0

[[row.cell]]
widget = "box"
title = "system"
title_color = "${theme.accent}"
border_color = "${theme.muted}"
padding = [0, 1]
child = { widget = "text", content = """
host ${data.system.hostname}
os   ${data.os.name}
""" }
"##;
    let frame = build_frame(parse(toml_text), &mock_registry());
    insta::assert_snapshot!("box_rounded_pipe", render_mode(&frame, ColorMode::None));
    insta::assert_snapshot!(
        "box_rounded_truecolor",
        render_mode(&frame, ColorMode::Truecolor)
    );
}

#[test]
fn gauge_widget_50_percent() {
    let toml_text = r##"
[layout]
gap = 0

[[row]]
gap = 0

[[row.cell]]
widget = "gauge"
label = "CPU"
value = "${data.cpu.usage}"
max = 100
width = 10
filled_char = "#"
empty_char = "-"
"##;
    let mut reg = mock_registry();
    reg.insert("cpu", serde_json::json!({ "usage": 50 }));
    let frame = build_frame(parse(toml_text), &reg);
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn show_if_false_collapses_cell() {
    let toml_text = r##"
[layout]
gap = 0

[[row]]
gap = 2

[[row.cell]]
widget = "text"
content = "left"

[[row.cell]]
widget = "text"
content = "middle"
show_if = "${data.flags.never}"

[[row.cell]]
widget = "text"
content = "right"
"##;
    let mut reg = mock_registry();
    reg.insert("flags", serde_json::json!({ "never": false }));
    let frame = build_frame(parse(toml_text), &reg);
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn show_if_true_renders() {
    let toml_text = r##"
[layout]
gap = 0

[[row]]
gap = 0

[[row.cell]]
widget = "text"
content = "shown"
show_if = "${data.flags.always}"
"##;
    let mut reg = mock_registry();
    reg.insert("flags", serde_json::json!({ "always": true }));
    let frame = build_frame(parse(toml_text), &reg);
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}

#[test]
fn pipe_output_contains_no_ansi() {
    let toml_text = r##"
[layout]
gap = 0

[theme]
accent = "#ff8800"

[[row]]
gap = 0

[[row.cell]]
widget = "text"
color = "${theme.accent}"
content = "should be plain"
"##;
    let frame = build_frame(parse(toml_text), &mock_registry());
    let rendered = render_mode(&frame, ColorMode::None);
    assert!(
        !rendered.contains('\x1b'),
        "pipe output must contain no ANSI escapes: {rendered:?}"
    );
}

#[test]
fn filter_chain_humanize() {
    let toml_text = r#"
[layout]
gap = 0

[[row]]
gap = 0

[[row.cell]]
widget = "text"
content = "up ${data.uptime|humanize}"
"#;
    let mut reg = mock_registry();
    reg.insert("uptime", json!(3661));

    let frame = build_frame(parse(toml_text), &reg);
    insta::assert_snapshot!(render_mode(&frame, ColorMode::None));
}
