//! Config-side colour specification.
//!
//! Accepts either a plain string (`color = "#ff8800"`, `color = "red"`,
//! `color = "${theme.accent}"`) or an inline-table gradient
//! (`color = { gradient = ["${theme.accent}", "#f38ba8"] }`).

use serde::{Deserialize, Serialize};

use crate::config::expr::{EvalContext, StaticContext, eval_single};
use crate::error::ConfigError;
use crate::style::{PaintSpec, Style, parse_color};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorSpec {
    Solid(String),
    Gradient { gradient: Vec<String> },
}

impl ColorSpec {
    /// Resolve to a [`PaintSpec`] using the build-time context.
    pub fn resolve(&self, ctx: &StaticContext) -> Result<PaintSpec, ConfigError> {
        match self {
            Self::Solid(raw) => {
                let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                let color =
                    parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?;
                Ok(PaintSpec::solid(color))
            }
            Self::Gradient { gradient } => {
                let mut stops = Vec::with_capacity(gradient.len());
                for raw in gradient {
                    let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                    if let Some(color) = parse_color(&resolved)
                        .map_err(|err| ConfigError::Invalid(err.to_string()))?
                    {
                        stops.push(color);
                    }
                }
                if stops.is_empty() {
                    return Err(ConfigError::Invalid(
                        "gradient has no resolvable color stops".into(),
                    ));
                }
                Ok(PaintSpec::gradient(stops))
            }
        }
    }
}

/// Convenience for widget configs that always take an optional colour:
/// resolve to `Option<PaintSpec>`, attaching the supplied attribute style
/// (bold/italic/etc.).
pub fn resolve_optional(
    spec: Option<&ColorSpec>,
    attrs: Style,
    ctx: &StaticContext,
) -> Result<PaintSpec, ConfigError> {
    let mut paint = match spec {
        Some(s) => s.resolve(ctx)?,
        None => PaintSpec::solid(None),
    };
    paint = paint.with_attrs(attrs);
    Ok(paint)
}
