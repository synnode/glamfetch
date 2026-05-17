//! `figlet` widget (spec §8) — render text as FIGlet ASCII art.
//!
//! Font resolution order:
//! 1. Filesystem path (string contains `/` or starts with `~`)
//! 2. Bundled `.flf` shipped in `fonts/` (case-insensitive, with or
//!    without the `.flf` suffix)
//! 3. `figlet-rs`'s built-in fonts (`standard`, `slant`, `small`, `big`)
//! 4. Fallback to `standard` with a warning
//!
//! The text template re-evaluates per render so `${data.*}` refs work in
//! `--watch` mode.

use std::path::PathBuf;

use figlet_rs::FIGlet;
use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigletConfig {
    pub text: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_font() -> String {
    "standard".into()
}

pub struct FigletWidget {
    text_template: String,
    font: FIGlet,
    style: Style,
}

// ---------------------------------------------------------------------------
// Bundled fonts (embedded at compile time)
// ---------------------------------------------------------------------------

macro_rules! bundled_fonts {
    ( $( $name:literal => $path:literal ),* $(,)? ) => {
        &[ $( ($name, include_str!($path)) ),* ]
    };
}

const BUNDLED: &[(&str, &str)] = bundled_fonts! {
    "ansi_shadow" => "../../fonts/ANSI_Shadow.flf",
    "shadow"      => "../../fonts/shadow.flf",
    "block"       => "../../fonts/block.flf",
    "mini"        => "../../fonts/mini.flf",
    "lean"        => "../../fonts/lean.flf",
    "script"      => "../../fonts/script.flf",
    "banner"      => "../../fonts/banner.flf",
};

impl FigletWidget {
    pub fn build(cfg: FigletConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let text_template = eval_template(&cfg.text, &EvalContext::build_only(ctx))?;
        let font = load_font(&cfg.font);
        let fg = match cfg.color.as_deref() {
            Some(raw) => {
                let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?
            }
            None => None,
        };
        Ok(Self {
            text_template,
            font,
            style: Style {
                fg,
                ..Style::plain()
            },
        })
    }
}

fn load_font(name: &str) -> FIGlet {
    // 1. Filesystem path
    if name.contains('/') || name.starts_with('~') {
        let path = expand_tilde(name);
        match std::fs::read_to_string(&path) {
            Ok(content) => match FIGlet::from_content(&content) {
                Ok(font) => return font,
                Err(err) => tracing::warn!(
                    "figlet: failed to parse `{}`: {err}, falling back to standard",
                    path.display()
                ),
            },
            Err(err) => tracing::warn!(
                "figlet: cannot read `{}`: {err}, falling back to standard",
                path.display()
            ),
        }
        return standard_font();
    }

    // 2. Bundled fonts (case-insensitive, ignore .flf suffix)
    let key = normalize_name(name);
    if let Some((_, content)) = BUNDLED.iter().find(|(n, _)| *n == key) {
        return FIGlet::from_content(content).expect("bundled FIGlet font parses");
    }

    // 3. figlet-rs's built-in convenience fonts
    let built_in = match key.as_str() {
        "standard" => Some(FIGlet::standard()),
        "slant" => Some(FIGlet::slant()),
        "small" => Some(FIGlet::small()),
        "big" => Some(FIGlet::big()),
        _ => None,
    };
    if let Some(Ok(font)) = built_in {
        return font;
    }

    // 4. Fallback
    tracing::warn!(
        "figlet: unknown font `{name}`, falling back to `standard`. Try one of: {}",
        available_font_list()
    );
    standard_font()
}

fn standard_font() -> FIGlet {
    FIGlet::standard().expect("standard figlet font loads")
}

fn normalize_name(name: &str) -> String {
    name.to_ascii_lowercase()
        .trim_end_matches(".flf")
        .to_string()
}

fn available_font_list() -> String {
    let mut names: Vec<&str> = BUNDLED.iter().map(|(n, _)| *n).collect();
    names.extend(["standard", "slant", "small", "big"]);
    names.sort_unstable();
    names.join(", ")
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(stripped);
    }
    PathBuf::from(path)
}

impl Widget for FigletWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let static_ctx = StaticContext::default();
        let ctx = EvalContext::full(&static_ctx, registry);
        let text = eval_template(&self.text_template, &ctx).map_err(|err| RenderError::Widget {
            widget: "figlet",
            message: err.to_string(),
        })?;

        let Some(figure) = self.font.convert(&text) else {
            return Ok(Cell::empty());
        };

        let raw = figure.to_string();
        let trimmed = raw.trim_end_matches('\n');

        let lines: Vec<StyledLine> = trimmed
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    StyledLine::empty()
                } else {
                    StyledLine::from_segments(vec![Segment::styled(line.to_string(), self.style)])
                }
            })
            .collect();

        Ok(Cell::from_lines(lines))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_names_are_lowercase_keys() {
        for (name, _) in BUNDLED {
            assert_eq!(*name, name.to_ascii_lowercase(), "{name}");
        }
    }

    #[test]
    fn normalize_strips_suffix_and_case() {
        assert_eq!(normalize_name("ANSI_Shadow.flf"), "ansi_shadow");
        assert_eq!(normalize_name("standard"), "standard");
    }

    #[test]
    fn all_bundled_fonts_parse() {
        for (name, content) in BUNDLED {
            FIGlet::from_content(content)
                .unwrap_or_else(|err| panic!("bundled font `{name}` failed to parse: {err}"));
        }
    }
}
