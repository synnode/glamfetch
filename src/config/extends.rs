//! `extends` chain resolution + deep merge (spec §6.6).
//!
//! Resolution order per reference:
//! 1. Built-in preset name (matches a key in [`builtin_preset`])
//! 2. Absolute path
//! 3. Path relative to the current file's directory
//! 4. Error
//!
//! Merge: maps are merged recursively (later overrides earlier), arrays
//! are *replaced* (matches CSS cascade / Nix overlay semantics — avoids
//! surprising `[[row]]` reordering when chaining presets).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::error::ConfigError;

const MAX_DEPTH: usize = 16;

/// Built-in presets shipped with the binary. Names land in
/// `--list-presets`.
pub const BUILTIN_PRESETS: &[(&str, &str)] = &[
    ("default", include_str!("../../presets/default.toml")),
    ("catppuccin", include_str!("../../presets/catppuccin.toml")),
    ("gruvbox", include_str!("../../presets/gruvbox.toml")),
    ("nord", include_str!("../../presets/nord.toml")),
];

pub fn builtin_preset(name: &str) -> Option<&'static str> {
    BUILTIN_PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, body)| *body)
}

/// Recursively expand `extends` for the given root config value. Returns
/// the fully merged toml table, with the original `extends` key stripped.
///
/// `origin_dir` is the directory the config was loaded from (used for
/// relative-path resolution). `<embedded>` for non-file sources.
pub fn expand(root: Value, origin_dir: Option<&Path>) -> Result<Value, ConfigError> {
    let mut visiting = HashSet::new();
    expand_inner(root, origin_dir, &mut visiting, 0)
}

fn expand_inner(
    mut value: Value,
    origin_dir: Option<&Path>,
    visiting: &mut HashSet<String>,
    depth: usize,
) -> Result<Value, ConfigError> {
    if depth > MAX_DEPTH {
        return Err(ConfigError::Invalid(format!(
            "extends chain exceeded {MAX_DEPTH} levels — possible cycle"
        )));
    }

    let Some(table) = value.as_table_mut() else {
        return Ok(value);
    };

    let Some(extends_value) = table.remove("extends") else {
        return Ok(value);
    };

    let refs = parse_extends(&extends_value)?;

    // Each ref resolves to a base config that itself may have `extends`.
    // Apply CSS-cascade order: leftmost is the lowest layer, rightmost
    // overrides earlier, and the current `value` overrides them all.
    let mut accumulated = Value::Table(toml::map::Map::new());
    for reference in refs {
        if !visiting.insert(reference.clone()) {
            return Err(ConfigError::Invalid(format!(
                "cycle in `extends`: `{reference}` references itself transitively"
            )));
        }

        let (base_text, base_dir) = load_extends_source(&reference, origin_dir)?;
        let base_parsed: Value = toml::from_str(&base_text).map_err(|err| {
            ConfigError::Invalid(format!("parsing extends target `{reference}`: {err}"))
        })?;
        let base_expanded = expand_inner(base_parsed, base_dir.as_deref(), visiting, depth + 1)?;

        accumulated = deep_merge(accumulated, base_expanded);

        visiting.remove(&reference);
    }

    Ok(deep_merge(accumulated, value))
}

fn parse_extends(value: &Value) -> Result<Vec<String>, ConfigError> {
    match value {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for entry in arr {
                let Value::String(s) = entry else {
                    return Err(ConfigError::Invalid(format!(
                        "`extends` array entries must be strings, got {entry:?}"
                    )));
                };
                out.push(s.clone());
            }
            Ok(out)
        }
        other => Err(ConfigError::Invalid(format!(
            "`extends` must be a string or array of strings, got {other:?}"
        ))),
    }
}

fn load_extends_source(
    reference: &str,
    origin_dir: Option<&Path>,
) -> Result<(String, Option<PathBuf>), ConfigError> {
    // 1. Built-in preset name (no path separator)
    if !reference.contains('/')
        && !reference.contains('\\')
        && let Some(body) = builtin_preset(reference)
    {
        return Ok((body.to_string(), None));
    }

    // 2. Absolute path
    let as_path = Path::new(reference);
    if as_path.is_absolute() {
        let text = std::fs::read_to_string(as_path)
            .map_err(|err| ConfigError::Invalid(format!("reading extends `{reference}`: {err}")))?;
        let dir = as_path.parent().map(Path::to_path_buf);
        return Ok((text, dir));
    }

    // 3. Relative to origin
    if let Some(dir) = origin_dir {
        let resolved = dir.join(as_path);
        if let Ok(text) = std::fs::read_to_string(&resolved) {
            let new_dir = resolved.parent().map(Path::to_path_buf);
            return Ok((text, new_dir));
        }
    }

    Err(ConfigError::ExtendsNotFound(reference.to_string()))
}

/// Deep-merge `over` onto `base`. Tables merge recursively; everything
/// else is overwrite. Arrays are *replaced* (not concatenated).
pub fn deep_merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Table(mut base_map), Value::Table(over_map)) => {
            for (key, over_value) in over_map {
                let merged = match base_map.remove(&key) {
                    Some(base_value) => deep_merge(base_value, over_value),
                    None => over_value,
                };
                base_map.insert(key, merged);
            }
            Value::Table(base_map)
        }
        (_, over) => over,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Value {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn merge_overrides_scalars() {
        let base = parse("a = 1\nb = 2");
        let over = parse("a = 99");
        let result = deep_merge(base, over);
        assert_eq!(result["a"].as_integer(), Some(99));
        assert_eq!(result["b"].as_integer(), Some(2));
    }

    #[test]
    fn merge_recurses_into_tables() {
        let base = parse("[theme]\naccent = \"red\"\nmuted = \"gray\"");
        let over = parse("[theme]\naccent = \"blue\"");
        let result = deep_merge(base, over);
        assert_eq!(result["theme"]["accent"].as_str(), Some("blue"));
        assert_eq!(result["theme"]["muted"].as_str(), Some("gray"));
    }

    #[test]
    fn merge_replaces_arrays() {
        let base = parse("xs = [1, 2, 3]");
        let over = parse("xs = [9]");
        let result = deep_merge(base, over);
        let arr = result["xs"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_integer(), Some(9));
    }

    #[test]
    fn expand_resolves_builtin_preset_and_overrides() {
        // catppuccin preset must exist as a built-in.
        let text = "extends = \"catppuccin\"\n[theme]\naccent = \"#ffffff\"\n";
        let value: Value = toml::from_str(text).unwrap();
        let expanded = expand(value, None).unwrap();
        // Theme.accent overridden, but other theme vars (e.g. muted) should
        // come from the catppuccin base.
        assert_eq!(expanded["theme"]["accent"].as_str(), Some("#ffffff"));
        assert!(expanded["theme"].as_table().unwrap().len() > 1);
    }
}
