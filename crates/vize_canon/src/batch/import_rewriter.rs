//! Import rewriter for transforming .vue imports to .vue.ts.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use vize_carton::{String, ToCompactString, cstr};

#[path = "import_rewriter_authored_vue_ts.rs"]
mod authored_vue_ts;

#[path = "import_rewriter_policy.rs"]
mod policy;

#[path = "import_rewriter_collect.rs"]
mod collect;
use collect::{ModuleSpecifierCollector, collect_specifier_occurrences};

#[path = "import_rewriter_virtual.rs"]
mod virtual_rewrite;
pub(super) use virtual_rewrite::rewrite_relative_vue_specifier;

#[path = "import_rewriter_virtual_alias.rs"]
mod virtual_alias;
pub(crate) use virtual_alias::VirtualAliasRewritePolicy;

#[path = "import_rewriter_dts.rs"]
mod dts_rewrite;

#[path = "import_rewriter_virtual_project.rs"]
mod virtual_project_rewrite;

#[path = "import_rewriter_source_map.rs"]
mod source_map;
pub use source_map::{ImportSourceMap, OffsetAdjustment, RewriteResult};

pub struct ImportRewriter;

impl ImportRewriter {
    pub fn new() -> Self {
        Self
    }

    /// Rewrite a generated module's `.vue` specifiers onto their mirror
    /// modules. `source_dir` (the authored file's directory, when known) also
    /// redirects a relative extensionless specifier whose target is a `.vue`
    /// file on disk (`./components/svg` for `svg.vue`, #3329).
    pub fn rewrite(
        &self,
        source: &str,
        source_type: SourceType,
        source_dir: Option<&Path>,
    ) -> RewriteResult {
        self.rewrite_with_missing_vue_policy(source, source_type, source_dir, true)
    }

    pub(crate) fn rewrite_with_missing_vue_policy(
        &self,
        source: &str,
        source_type: SourceType,
        source_dir: Option<&Path>,
        preserve_missing_vue_diagnostics: bool,
    ) -> RewriteResult {
        self.rewrite_with_missing_vue_policy_and_alias_policy(
            source,
            source_type,
            source_dir,
            preserve_missing_vue_diagnostics,
            None,
        )
    }

    pub(crate) fn rewrite_with_missing_vue_policy_and_alias_policy(
        &self,
        source: &str,
        source_type: SourceType,
        source_dir: Option<&Path>,
        preserve_missing_vue_diagnostics: bool,
        alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    ) -> RewriteResult {
        let relative_candidate =
            source_dir.is_some() && source_may_contain_relative_specifier(source);
        if !source.contains(".vue") && !relative_candidate {
            return RewriteResult {
                code: source.to_compact_string(),
                source_map: ImportSourceMap::empty(),
            };
        }

        self.rewrite_with(source, source_type, |path, _| {
            self.rewrite_module_specifier_with_missing_vue_policy_and_alias_policy(
                path,
                source_dir,
                preserve_missing_vue_diagnostics,
                true,
                alias_rewrite_policy,
            )
            .or_else(|| source_dir.and_then(|dir| rewrite_relative_vue_specifier(path, dir)))
        })
    }

    pub fn rewrite_declaration_specifiers(
        &self,
        source: &str,
        source_type: SourceType,
    ) -> RewriteResult {
        if !source.contains(".vue.ts") && !source.contains(".vue.tsx") {
            return RewriteResult {
                code: source.to_compact_string(),
                source_map: ImportSourceMap::empty(),
            };
        }

        self.rewrite_with(source, source_type, |path, _| {
            self.rewrite_declaration_specifier(path)
        })
    }

    pub(super) fn rewrite_with<F>(
        &self,
        source: &str,
        source_type: SourceType,
        rewrite_specifier: F,
    ) -> RewriteResult
    where
        F: Fn(&str, crate::PackageResolutionMode) -> Option<String>,
    {
        let allocator = Allocator::default();
        let parser = Parser::new(&allocator, source, source_type);
        let result = parser.parse();

        let mut collector = ModuleSpecifierCollector::new();
        collector.visit_program(&result.program);

        let mut rewrites: Vec<(u32, u32, String)> = Vec::new();
        for (start, end, path, mode) in collector.specifiers {
            if let Some(rewrite) = rewrite_specifier(&path, mode) {
                rewrites.push((start, end, rewrite));
            }
        }

        rewrites.sort_by_key(|rewrite| std::cmp::Reverse(rewrite.0));

        let mut output = source.to_compact_string();
        let mut adjustments = Vec::new();

        for (start, end, new_path) in rewrites {
            let original_len = (end - start) as i32;
            let new_len = new_path.len() as i32;

            output.replace_range(start as usize..end as usize, new_path.as_str());

            adjustments.push(OffsetAdjustment {
                original_offset: start,
                adjustment: new_len - original_len,
            });
        }

        adjustments.reverse();

        RewriteResult {
            code: output,
            source_map: ImportSourceMap::new(adjustments),
        }
    }

    /// Relative SFC dependencies of `source`, always spelled with the `.vue`
    /// extension. With `source_dir`, extensionless specifiers whose target is a
    /// `.vue` file are reported too, so the caller opens the dependency the
    /// rewriter redirects them to (#3329).
    pub fn collect_relative_vue_specifiers(
        &self,
        source: &str,
        source_type: SourceType,
        source_dir: Option<&Path>,
    ) -> Vec<String> {
        if !source.contains(".vue") && source_dir.is_none() {
            return Vec::new();
        }

        let allocator = Allocator::default();
        let parser = Parser::new(&allocator, source, source_type);
        let result = parser.parse();

        let mut specifiers: Vec<String> = Vec::new();
        let mut collector = ModuleSpecifierCollector::new();
        collector.visit_program(&result.program);
        for (_, _, path, _) in collector.specifiers {
            let candidate =
                if path.ends_with(".vue") && (path.starts_with("./") || path.starts_with("../")) {
                    path.to_compact_string()
                } else if source_dir
                    .is_some_and(|dir| rewrite_relative_vue_specifier(&path, dir).is_some())
                {
                    cstr!("{path}.vue")
                } else {
                    continue;
                };
            if !specifiers.iter().any(|s| s.as_str() == candidate.as_str()) {
                specifiers.push(candidate);
            }
        }

        specifiers
    }

    /// Every module specifier in `source`, in order, without filtering —
    /// the reachability pass (#3887) resolves them itself.
    pub(crate) fn collect_all_specifiers(
        &self,
        source: &str,
        source_type: SourceType,
    ) -> Vec<String> {
        let allocator = Allocator::default();
        let parser = Parser::new(&allocator, source, source_type);
        let result = parser.parse();
        let mut collector = ModuleSpecifierCollector::new();
        collector.visit_program(&result.program);
        let mut specifiers: Vec<String> = Vec::new();
        for (_, _, path, _) in collector.specifiers {
            if !specifiers.contains(&path) {
                specifiers.push(path);
            }
        }
        specifiers
    }

    /// Every module specifier with the syntactic import/require occurrence
    /// mode used in TypeScript's package-resolution cache identity.
    pub(crate) fn collect_all_specifier_occurrences(
        &self,
        source: &str,
        source_type: SourceType,
    ) -> Vec<(String, crate::PackageResolutionMode)> {
        collect_specifier_occurrences(source, source_type)
    }

    fn rewrite_declaration_specifier(&self, path: &str) -> Option<String> {
        if path.ends_with(".vue.tsx") {
            return path
                .strip_suffix(".tsx")
                .map(|value| value.to_compact_string());
        }
        if path.ends_with(".vue.ts") {
            return path
                .strip_suffix(".ts")
                .map(|value| value.to_compact_string());
        }
        None
    }
}

pub(crate) fn source_may_contain_relative_specifier(source: &str) -> bool {
    ["'./", "\"./", "'../", "\"../"]
        .iter()
        .any(|needle| source.contains(needle))
}

impl Default for ImportRewriter {
    fn default() -> Self {
        Self::new()
    }
}
