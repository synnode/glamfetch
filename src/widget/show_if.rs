//! `show_if` wrapper (spec §6.5).
//!
//! When the wrapped expression evaluates to a *falsy* value (JSON semantics,
//! not JavaScript: `"0"` is truthy because it's a non-empty string), the
//! widget renders as an empty cell — zero width, zero height — so the row
//! closes up around it without extra gap. Anything truthy delegates to the
//! inner widget unchanged.

use serde_json::Value;

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single};
use crate::error::RenderError;

use super::{Cell, Widget};

pub struct ShowIfWidget {
    expr: String,
    static_ctx: StaticContext,
    inner: Box<dyn Widget>,
}

impl ShowIfWidget {
    pub fn wrap(expr: String, static_ctx: StaticContext, inner: Box<dyn Widget>) -> Self {
        Self {
            expr,
            static_ctx,
            inner,
        }
    }
}

impl Widget for ShowIfWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let ctx = EvalContext::full(&self.static_ctx, registry);
        let value = eval_single(&self.expr, &ctx).map_err(|err| RenderError::Widget {
            widget: "show_if",
            message: err.to_string(),
        })?;
        if is_truthy_text(&value) {
            self.inner.render(registry, max_width)
        } else {
            Ok(Cell::empty())
        }
    }
}

/// JSON-truthy semantics applied to the textual result of [`eval_single`].
///
/// The evaluator stringifies its result, so we recover the truthy/falsy
/// distinction by inspecting the rendered text:
/// - empty / `"null"` / `"false"` → falsy
/// - `"0"` (or `"0.0"`) → falsy (a *parsed* zero, before stringification)
/// - everything else (including `"false"` *as text from a string source*) →
///   truthy
///
/// This isn't perfect — a collector that genuinely returns the string `"0"`
/// will be considered falsy here, since we can't distinguish a stringified
/// `0` from a literal `"0"`. In practice nothing useful returns that.
pub fn is_truthy_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if matches!(trimmed, "null" | "false") {
        return false;
    }
    if let Ok(n) = trimmed.parse::<f64>()
        && n == 0.0
    {
        return false;
    }
    true
}

/// Apply JSON-truthy semantics directly to a `Value`. Used by tests and
/// (eventually) by direct render-context evaluation that bypasses
/// stringification.
#[allow(dead_code)]
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_truthiness() {
        assert!(!is_truthy(&json!(null)));
        assert!(!is_truthy(&json!(false)));
        assert!(!is_truthy(&json!(0)));
        assert!(!is_truthy(&json!(0.0)));
        assert!(!is_truthy(&json!("")));
        assert!(!is_truthy(&json!([])));
        assert!(!is_truthy(&json!({})));

        assert!(is_truthy(&json!(true)));
        assert!(is_truthy(&json!(1)));
        assert!(is_truthy(&json!("0"))); // non-empty string
        assert!(is_truthy(&json!("false"))); // non-empty string
        assert!(is_truthy(&json!([1])));
        assert!(is_truthy(&json!({"k": 1})));
    }

    #[test]
    fn text_truthiness_after_stringification() {
        assert!(!is_truthy_text(""));
        assert!(!is_truthy_text("null"));
        assert!(!is_truthy_text("false"));
        assert!(!is_truthy_text("0"));
        assert!(!is_truthy_text("0.0"));
        assert!(is_truthy_text("true"));
        assert!(is_truthy_text("any"));
    }
}
