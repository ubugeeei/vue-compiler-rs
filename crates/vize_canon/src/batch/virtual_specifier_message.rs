//! Restores authored Vue module specifiers in TypeScript diagnostics.

use vize_carton::{String, ToCompactString, cstr};

/// Suffix added to an unresolved authored `.vue.ts`/`.vue.tsx` import so
/// TypeScript cannot accidentally resolve the generated SFC mirror.
pub const AUTHORED_VUE_TS_SENTINEL: &str = "/__vize_authored_vue_ts__";
pub(crate) const AUTHORED_VUE_TS_ALIAS_SENTINEL: &str = ".__vize_authored_vue_ts_alias__";
pub(crate) const MISSING_VUE_IMPORT_SENTINEL: &str = "/__vize_missing_vue_import__";

/// The quote pairs a checker message may wrap a specifier in.
pub(crate) const QUOTE_PAIRS: [(char, char); 3] =
    [('\'', '\''), ('"', '"'), ('\u{2018}', '\u{2019}')];

/// Restore virtual Vue module specifiers quoted in a TypeScript diagnostic.
///
/// Replacement requires proof that the reported spelling is absent from the
/// authored source. This preserves diagnostics for real `.vue.ts` files and
/// even for a deliberately authored sentinel-like module name.
pub fn restore_virtual_vue_specifiers(message: &str, authored_source: &str) -> String {
    let mut rewrites = Vec::new();
    for reported in quoted_specifiers(message) {
        if authored_source.contains(reported) {
            continue;
        }
        let Some(authored) = authored_specifier(reported) else {
            continue;
        };
        rewrites.push((reported.to_compact_string(), authored.to_compact_string()));
    }

    let mut rewritten = message.to_compact_string();
    for (reported, authored) in rewrites {
        for (open, close) in QUOTE_PAIRS {
            let quoted = cstr!("{open}{reported}{close}");
            let restored = cstr!("{open}{authored}{close}");
            rewritten = rewritten.replace(quoted.as_str(), restored.as_str()).into();
        }
    }
    rewritten
}

fn authored_specifier(reported: &str) -> Option<&str> {
    if let Some(authored) = reported.strip_suffix(MISSING_VUE_IMPORT_SENTINEL)
        && let Some(authored) = authored
            .strip_suffix(".ts")
            .or_else(|| authored.strip_suffix(".tsx"))
        && authored.ends_with(".vue")
    {
        return Some(authored);
    }
    for marker in [AUTHORED_VUE_TS_SENTINEL, AUTHORED_VUE_TS_ALIAS_SENTINEL] {
        if let Some(authored) = reported.strip_suffix(marker)
            && (authored.ends_with(".vue.ts") || authored.ends_with(".vue.tsx"))
        {
            return Some(authored);
        }
    }
    reported
        .strip_suffix(".ts")
        .or_else(|| reported.strip_suffix(".tsx"))
        .filter(|authored| authored.ends_with(".vue"))
}

/// Every distinct specifier-shaped run quoted in `message`.
///
/// Each quote pair is scanned in its own pass so a specifier nested inside
/// another quoted run (`Module '"./Panel.vue.ts"' ...`) is still found. The
/// result is therefore grouped by [`QUOTE_PAIRS`] order, not by position in
/// `message`: a double-quoted specifier precedes a single-quoted one only if
/// its quote pair comes first. Both callers rewrite every returned specifier
/// across the whole message, so order carries no meaning beyond determinism.
///
/// A candidate must look like a specifier — no whitespace and no quote
/// characters — so an unbalanced pairing can never yield a run of prose.
pub(crate) fn quoted_specifiers(message: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for (open, close) in QUOTE_PAIRS {
        let mut rest = message;
        while let Some(start) = rest.find(open) {
            let after_open = &rest[start + open.len_utf8()..];
            let Some(end) = after_open.find(close) else {
                break;
            };
            let candidate = &after_open[..end];
            if is_specifier_shaped(candidate) && !found.contains(&candidate) {
                found.push(candidate);
            }
            rest = &after_open[end + close.len_utf8()..];
        }
    }
    found
}

fn is_specifier_shaped(candidate: &str) -> bool {
    !candidate.is_empty()
        && !candidate
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '\'' | '"' | '`'))
}

#[cfg(test)]
mod tests {
    use super::{
        AUTHORED_VUE_TS_ALIAS_SENTINEL, AUTHORED_VUE_TS_SENTINEL, MISSING_VUE_IMPORT_SENTINEL,
        quoted_specifiers, restore_virtual_vue_specifiers,
    };

    #[test]
    fn restores_generated_mirrors_and_authored_collision_markers() {
        let marker = AUTHORED_VUE_TS_SENTINEL;
        let alias = AUTHORED_VUE_TS_ALIAS_SENTINEL;
        let message = format!(
            "Module '\"./Panel.vue.ts\"' failed; cannot find '../Missing.vue.ts{marker}' or './Typed.vue.ts{alias}'."
        );
        assert_eq!(
            restore_virtual_vue_specifiers(
                &message,
                "import './Panel.vue'; import '../Missing.vue.ts'"
            ),
            "Module '\"./Panel.vue\"' failed; cannot find '../Missing.vue.ts' or './Typed.vue.ts'."
        );
    }

    #[test]
    fn preserves_authored_typescript_and_marker_like_specifiers() {
        let marker = AUTHORED_VUE_TS_SENTINEL;
        let reported = format!("./Literal.vue.ts{marker}");
        let message = format!("Cannot find module '{reported}'. Also './Authored.vue.ts'.");
        let source = format!("import '{reported}'; import './Authored.vue.ts';");
        assert_eq!(restore_virtual_vue_specifiers(&message, &source), message);
    }

    #[test]
    fn restores_missing_generated_vue_imports_to_authored_sfc_specifiers() {
        let marker = MISSING_VUE_IMPORT_SENTINEL;
        let message = format!("Cannot find module './Missing.vue.ts{marker}'.");
        assert_eq!(
            restore_virtual_vue_specifiers(&message, "import './Missing.vue';"),
            "Cannot find module './Missing.vue'."
        );
    }

    #[test]
    fn ignores_unquoted_suffixes_and_unrelated_modules() {
        let message = "Generated ./Panel.vue.ts; cannot find './util.ts'.";
        assert_eq!(restore_virtual_vue_specifiers(message, ""), message);
    }

    #[test]
    fn scans_mixed_quote_styles_grouped_by_quote_pair() {
        let message = "Module \"./First.vue.ts\" and './Second.vue.ts' differ.";
        assert_eq!(
            quoted_specifiers(message),
            ["./Second.vue.ts", "./First.vue.ts"],
            "runs are grouped by QUOTE_PAIRS order, not by position in the message"
        );
        assert_eq!(
            restore_virtual_vue_specifiers(message, "import './First.vue'; import './Second.vue';"),
            "Module \"./First.vue\" and './Second.vue' differ.",
            "every returned specifier is rewritten regardless of scan order"
        );
    }
}
