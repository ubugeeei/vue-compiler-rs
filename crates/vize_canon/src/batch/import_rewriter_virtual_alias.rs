//! Virtual-project policy for aliased `.vue` specifier rewrites.
//!
//! The generic rewriter rewrites `@/Foo.vue` to `@/Foo.vue.ts`, which is correct
//! only when the virtual tsconfig carries a matching `paths` alias. Without one,
//! TypeScript and vue-tsc resolve the authored specifier as-is; appending `.ts`
//! changes the unresolved target and can introduce false TS2307 diagnostics.

use serde_json::{Map, Value};

#[derive(Debug, Default)]
pub(crate) struct VirtualAliasRewritePolicy {
    patterns: Vec<PathAliasPattern>,
}

#[derive(Debug)]
struct PathAliasPattern {
    prefix: std::string::String,
    suffix: std::string::String,
    wildcard: bool,
}

impl VirtualAliasRewritePolicy {
    #[allow(clippy::disallowed_types)]
    pub(crate) fn from_paths(paths: &Map<std::string::String, Value>) -> Self {
        let patterns = paths
            .iter()
            .filter(|(_, targets)| targets_enable_first_party_alias(targets))
            .map(|(pattern, _)| PathAliasPattern::new(pattern))
            .collect();
        Self { patterns }
    }

    pub(crate) fn should_rewrite_vue_specifier(&self, specifier: &str) -> bool {
        if !is_policy_controlled_vue_specifier(specifier) {
            return true;
        }
        self.matches(&format!("{specifier}.ts"))
    }

    fn matches(&self, specifier: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.matches(specifier))
    }
}

impl PathAliasPattern {
    fn new(pattern: &str) -> Self {
        let Some(wildcard) = pattern.find('*') else {
            return Self {
                prefix: pattern.to_owned(),
                suffix: std::string::String::new(),
                wildcard: false,
            };
        };
        Self {
            prefix: pattern[..wildcard].to_owned(),
            suffix: pattern[wildcard + 1..].to_owned(),
            wildcard: true,
        }
    }

    fn matches(&self, specifier: &str) -> bool {
        if self.wildcard {
            specifier.starts_with(self.prefix.as_str())
                && specifier.ends_with(self.suffix.as_str())
                && specifier.len() >= self.prefix.len() + self.suffix.len()
        } else {
            specifier == self.prefix
        }
    }
}

fn is_policy_controlled_vue_specifier(specifier: &str) -> bool {
    specifier.ends_with(".vue") && (specifier.starts_with("@/") || specifier.starts_with("~/"))
}

fn targets_enable_first_party_alias(targets: &Value) -> bool {
    let Some(targets) = targets.as_array() else {
        return false;
    };
    targets
        .iter()
        .filter_map(Value::as_str)
        .any(|target| !target.is_empty() && !target.contains("node_modules"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::VirtualAliasRewritePolicy;

    fn policy(paths: serde_json::Value) -> VirtualAliasRewritePolicy {
        VirtualAliasRewritePolicy::from_paths(paths.as_object().unwrap())
    }

    #[test]
    fn aliased_vue_specifier_requires_a_matching_path_alias() {
        let empty = policy(json!({}));
        assert!(!empty.should_rewrite_vue_specifier("@/App.vue"));
        assert!(!empty.should_rewrite_vue_specifier("~/App.vue"));
        assert!(empty.should_rewrite_vue_specifier("./App.vue"));

        let configured = policy(json!({
            "@/*": ["src/*"],
            "~/*": ["app/*"],
            "#/*.vue": ["src/*.vue"]
        }));
        assert!(configured.should_rewrite_vue_specifier("@/App.vue"));
        assert!(configured.should_rewrite_vue_specifier("~/App.vue"));
        assert!(configured.should_rewrite_vue_specifier("#/App.vue"));
    }

    #[test]
    fn vue_suffixed_alias_keys_do_not_claim_rewritten_specifiers() {
        let configured = policy(json!({
            "@/*.vue": ["src/*.vue"]
        }));

        assert!(!configured.should_rewrite_vue_specifier("@/App.vue"));
    }

    #[test]
    fn dependency_only_aliases_do_not_claim_first_party_vue_specifiers() {
        let configured = policy(json!({
            "@/*": ["node_modules/@scope/*"]
        }));

        assert!(!configured.should_rewrite_vue_specifier("@/App.vue"));
    }

    #[test]
    fn empty_and_non_string_targets_do_not_claim_first_party_aliases() {
        let configured = policy(json!({
            "@/*": ["", 42, null]
        }));

        assert!(!configured.should_rewrite_vue_specifier("@/App.vue"));
    }

    #[test]
    fn a_mixed_target_list_can_still_claim_a_first_party_alias() {
        let configured = policy(json!({
            "@/*": ["node_modules/@scope/*", "src/*"]
        }));

        assert!(configured.should_rewrite_vue_specifier("@/App.vue"));
    }
}
