//! Theme variable resolution (spec §6.2 / §6.3).
//!
//! Raw `[theme]` values may reference other theme vars
//! (`border_fg = "${theme.muted}"`). [`resolve`] walks the map until all
//! `${theme.*}` references collapse to concrete strings, erroring on
//! undefined targets or cycles.

use std::collections::{BTreeMap, HashSet};

use crate::error::ConfigError;

const MAX_PASSES: usize = 16;

pub type ThemeMap = BTreeMap<String, String>;

/// Resolve every `${theme.foo}` reference inside the value strings.
///
/// Only `${theme.*}` is resolved here. Other namespaces (`data`, `icons`,
/// `env`) are left intact for the main evaluator to handle at template time.
pub fn resolve(raw: &ThemeMap) -> Result<ThemeMap, ConfigError> {
    let mut current: ThemeMap = raw.clone();

    for _ in 0..MAX_PASSES {
        let mut next = ThemeMap::new();
        let mut changed = false;

        for (key, value) in &current {
            let resolved = expand_theme_refs(value, &current, &mut HashSet::new(), key)?;
            if &resolved != value {
                changed = true;
            }
            next.insert(key.clone(), resolved);
        }

        current = next;
        if !changed {
            // No further changes — verify everything is fully resolved.
            for (key, value) in &current {
                if contains_unresolved_theme(value) {
                    return Err(ConfigError::Invalid(format!(
                        "theme variable `{key}` still contains an unresolved `${{theme.*}}` reference: `{value}`"
                    )));
                }
            }
            return Ok(current);
        }
    }

    Err(ConfigError::Invalid(
        "theme variable resolution did not converge — possible cycle".into(),
    ))
}

fn expand_theme_refs(
    input: &str,
    map: &ThemeMap,
    visiting: &mut HashSet<String>,
    owner: &str,
) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("${theme.") {
        out.push_str(&rest[..start]);
        rest = &rest[start + "${theme.".len()..];

        let Some(end) = rest.find('}') else {
            // Unterminated — pass through.
            out.push_str("${theme.");
            out.push_str(rest);
            return Ok(out);
        };

        let var_name = &rest[..end];
        rest = &rest[end + 1..];

        if var_name == owner || !visiting.insert(var_name.to_string()) {
            return Err(ConfigError::Invalid(format!(
                "cycle detected resolving theme variable `{owner}` (chain through `{var_name}`)"
            )));
        }

        let value = map
            .get(var_name)
            .ok_or_else(|| ConfigError::UndefinedThemeVar(var_name.to_string()))?;

        let expanded = expand_theme_refs(value, map, visiting, owner)?;
        out.push_str(&expanded);

        visiting.remove(var_name);
    }

    out.push_str(rest);
    Ok(out)
}

fn contains_unresolved_theme(s: &str) -> bool {
    let mut rest = s;
    while let Some(idx) = rest.find("${theme.") {
        rest = &rest[idx + 1..];
        if rest.find('}').is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> ThemeMap {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    #[test]
    fn resolves_single_indirection() {
        let raw = map(&[("a", "#aaa"), ("b", "${theme.a}")]);
        let out = resolve(&raw).unwrap();
        assert_eq!(out.get("b").unwrap(), "#aaa");
    }

    #[test]
    fn resolves_chain() {
        let raw = map(&[("a", "#111"), ("b", "${theme.a}"), ("c", "${theme.b}")]);
        let out = resolve(&raw).unwrap();
        assert_eq!(out.get("c").unwrap(), "#111");
    }

    #[test]
    fn cycle_detected() {
        let raw = map(&[("a", "${theme.b}"), ("b", "${theme.a}")]);
        assert!(resolve(&raw).is_err());
    }

    #[test]
    fn self_cycle_detected() {
        let raw = map(&[("a", "${theme.a}")]);
        assert!(resolve(&raw).is_err());
    }

    #[test]
    fn undefined_errors() {
        let raw = map(&[("a", "${theme.missing}")]);
        assert!(matches!(
            resolve(&raw).unwrap_err(),
            ConfigError::UndefinedThemeVar(_)
        ));
    }

    #[test]
    fn passthrough_other_namespaces() {
        let raw = map(&[("a", "${data.cpu.usage}")]);
        let out = resolve(&raw).unwrap();
        assert_eq!(out.get("a").unwrap(), "${data.cpu.usage}");
    }
}
