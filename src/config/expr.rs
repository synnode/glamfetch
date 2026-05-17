//! Expression evaluator for `${namespace.path|filter|filter(arg)}` (spec §6.3).
//!
//! Two-phase resolution model:
//! - **Build-time** (theme/icons/env): a fully-resolved value is known when
//!   the widget tree is constructed. Refs to these namespaces collapse to
//!   constants at build time.
//! - **Render-time** (data): resolved each render against a
//!   [`CollectorRegistry`]. If the registry isn't available (build phase),
//!   `${data.*}` refs are left as literal text so a later render-time pass
//!   can resolve them.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::collect::CollectorRegistry;
use crate::error::ConfigError;

use super::filters::{Filter, value_to_display};

/// Static context known at config-load time.
#[derive(Debug, Default, Clone)]
pub struct StaticContext {
    pub theme: BTreeMap<String, String>,
    pub icons: BTreeMap<String, String>,
    pub env_allowed: bool,
}

/// Combined context for full evaluation (includes runtime registry).
pub struct EvalContext<'a> {
    pub statik: &'a StaticContext,
    pub registry: Option<&'a CollectorRegistry>,
}

impl<'a> EvalContext<'a> {
    pub fn build_only(statik: &'a StaticContext) -> Self {
        Self {
            statik,
            registry: None,
        }
    }

    pub fn full(statik: &'a StaticContext, registry: &'a CollectorRegistry) -> Self {
        Self {
            statik,
            registry: Some(registry),
        }
    }
}

/// Evaluate a template string. Returns the substituted text.
pub fn eval_template(input: &str, ctx: &EvalContext<'_>) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];

        let Some(end) = rest.find('}') else {
            out.push_str("${");
            out.push_str(rest);
            return Ok(out);
        };

        let expr = &rest[..end];
        rest = &rest[end + 1..];

        match eval_expr(expr, ctx)? {
            EvalResult::Resolved(s) => out.push_str(&s),
            EvalResult::Deferred => {
                out.push_str("${");
                out.push_str(expr);
                out.push('}');
            }
        }
    }

    out.push_str(rest);
    Ok(out)
}

/// Evaluate a value slot that may contain one or more `${...}` references
/// mixed with literal text (e.g. a color string `"${theme.accent}"`, a
/// plain `"#ff8800"`, or `"red"`). Strings without any `${` pass through
/// unchanged.
///
/// This is the public alias for [`eval_template`] used by widget builders
/// — exposed separately to document the intended call site, not to add
/// behavior. Use [`eval_template`] directly if you need to make it clear
/// the input is a template string.
pub fn eval_single(expr: &str, ctx: &EvalContext<'_>) -> Result<String, ConfigError> {
    eval_template(expr, ctx)
}

enum EvalResult {
    Resolved(String),
    Deferred,
}

fn eval_expr(expr: &str, ctx: &EvalContext<'_>) -> Result<EvalResult, ConfigError> {
    let (head, filters) = parse_pipeline(expr)?;
    let (namespace, path) = match head.split_once('.') {
        Some(pair) => pair,
        None => (head, ""),
    };

    let raw = match namespace {
        "theme" => match ctx.statik.theme.get(path) {
            Some(v) => Value::String(v.clone()),
            None => {
                return Err(ConfigError::UndefinedThemeVar(path.to_string()));
            }
        },
        "icons" => Value::String(ctx.statik.icons.get(path).cloned().unwrap_or_default()),
        "env" => {
            if !ctx.statik.env_allowed {
                Value::Null
            } else {
                Value::String(std::env::var(path).unwrap_or_default())
            }
        }
        "data" => match ctx.registry {
            None => return Ok(EvalResult::Deferred),
            Some(reg) => reg.get(path).cloned().unwrap_or(Value::Null),
        },
        other => {
            return Err(ConfigError::Invalid(format!(
                "unknown expression namespace `{other}` in `${{{expr}}}`"
            )));
        }
    };

    let mut current = raw;
    for filter in filters {
        current = filter.apply(current)?;
    }
    Ok(EvalResult::Resolved(value_to_display(&current)))
}

fn parse_pipeline(expr: &str) -> Result<(&str, Vec<Filter>), ConfigError> {
    let mut parts = expr.split('|');
    let head = parts.next().unwrap_or("").trim();
    let mut filters = Vec::new();
    for raw in parts {
        filters.push(parse_filter(raw.trim())?);
    }
    Ok((head, filters))
}

fn parse_filter(s: &str) -> Result<Filter, ConfigError> {
    let (name, args) = match s.find('(') {
        Some(open) => {
            let close = s
                .rfind(')')
                .ok_or_else(|| ConfigError::Invalid(format!("filter `{s}` missing closing `)`")))?;
            if close < open {
                return Err(ConfigError::Invalid(format!(
                    "filter `{s}`: malformed parentheses"
                )));
            }
            let name = s[..open].trim().to_string();
            let inside = &s[open + 1..close];
            let args = inside
                .split(',')
                .map(|a| a.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|a| !a.is_empty())
                .collect();
            (name, args)
        }
        None => (s.trim().to_string(), Vec::new()),
    };
    Ok(Filter { name, args })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> StaticContext {
        let mut theme = BTreeMap::new();
        theme.insert("accent".into(), "#ff8800".into());
        theme.insert("muted".into(), "#666666".into());
        let mut icons = BTreeMap::new();
        icons.insert("cpu".into(), "".into());
        StaticContext {
            theme,
            icons,
            env_allowed: false,
        }
    }

    #[test]
    fn substitutes_theme() {
        let s = ctx();
        let out = eval_template("color=${theme.accent}", &EvalContext::build_only(&s)).unwrap();
        assert_eq!(out, "color=#ff8800");
    }

    #[test]
    fn data_deferred_when_no_registry() {
        let s = ctx();
        let out = eval_template("${data.cpu.usage}", &EvalContext::build_only(&s)).unwrap();
        assert_eq!(out, "${data.cpu.usage}");
    }

    #[test]
    fn data_resolved_with_registry() {
        let s = ctx();
        let mut reg = CollectorRegistry::new();
        reg.insert("cpu", json!({ "usage": 42 }));
        let out = eval_template("u=${data.cpu.usage}", &EvalContext::full(&s, &reg)).unwrap();
        assert_eq!(out, "u=42");
    }

    #[test]
    fn filter_chain_applies() {
        let s = ctx();
        let mut reg = CollectorRegistry::new();
        reg.insert("u", json!(3661));
        let out = eval_template("${data.u|humanize}", &EvalContext::full(&s, &reg)).unwrap();
        assert_eq!(out, "1h 1m");
    }

    #[test]
    fn filter_with_arg() {
        let s = ctx();
        let mut reg = CollectorRegistry::new();
        reg.insert("u", json!(1.23456));
        let out = eval_template("${data.u|round(2)}", &EvalContext::full(&s, &reg)).unwrap();
        assert_eq!(out, "1.23");
    }

    #[test]
    fn undefined_theme_errors() {
        let s = ctx();
        let err = eval_template("${theme.nope}", &EvalContext::build_only(&s)).unwrap_err();
        assert!(matches!(err, ConfigError::UndefinedThemeVar(_)));
    }

    #[test]
    fn icons_lookup() {
        let s = ctx();
        let out = eval_template("${icons.cpu}", &EvalContext::build_only(&s)).unwrap();
        assert_eq!(out, "");
    }
}
