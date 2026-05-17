//! Pre-pass: extract referenced `${data.<root>}` collector roots.
//!
//! Spec §4.3 / §11: skip collectors that nothing in the layout reads. The
//! cheapest correct strategy is to scan the raw config text for the literal
//! pattern `${data.<word>`. False positives (e.g. inside comments) only
//! widen the set, which is safe — we'd just run a collector we didn't need.

use std::collections::HashSet;

const PREFIX: &str = "${data.";

/// Walk the text looking for `${data.<root>` occurrences and collect each
/// root identifier. The match stops at the first non-`[A-Za-z0-9_]` char.
pub fn referenced_data_roots(text: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut rest = text;
    while let Some(idx) = rest.find(PREFIX) {
        rest = &rest[idx + PREFIX.len()..];
        let end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if end > 0 {
            found.insert(rest[..end].to_string());
        }
        rest = &rest[end..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> HashSet<String> {
        referenced_data_roots(text)
    }

    #[test]
    fn finds_single_root() {
        let set = run("content = \"host=${data.system.hostname}\"");
        assert!(set.contains("system"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn finds_multiple_distinct_roots() {
        let set = run("a=${data.cpu.usage} b=${data.mem.percent} c=${data.cpu.cores}");
        assert!(set.contains("cpu"));
        assert!(set.contains("mem"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn ignores_theme_and_other_namespaces() {
        let set = run("color=${theme.accent} env=${env.USER}");
        assert!(set.is_empty());
    }

    #[test]
    fn handles_trailing_brace_only() {
        let set = run("${data.uptime}");
        assert!(set.contains("uptime"));
    }
}
