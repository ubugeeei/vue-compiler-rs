use std::path::Path;

use oxc_span::SourceType;
use vize_carton::{ToCompactString, cstr};

use super::{
    ImportRewriter, ImportSourceMap, RewriteResult, VirtualAliasRewritePolicy, authored_vue_ts,
    dts_rewrite::rewrite_relative_dts_specifier,
    source_may_contain_relative_specifier,
    virtual_rewrite::{
        absolute_import_needs_virtual_rewrite, is_rewritable_project_specifier,
        is_rewritable_vue_specifier, rewrite_relative_vue_specifier,
    },
};

impl ImportRewriter {
    /// Rewrite a script's module specifiers for the canon virtual project.
    /// `source_dir` (when known) enables the generated-`.d.ts` redirect (#2227).
    pub fn rewrite_for_virtual_project(
        &self,
        source: &str,
        source_type: SourceType,
        roots: (&Path, &Path),
        source_dir: Option<&Path>,
    ) -> RewriteResult {
        self.rewrite_for_virtual_project_with_policy(
            source,
            source_type,
            roots,
            source_dir,
            false,
            None,
        )
    }

    pub(crate) fn rewrite_for_virtual_project_with_alias_policy(
        &self,
        source: &str,
        source_type: SourceType,
        roots: (&Path, &Path),
        source_dir: Option<&Path>,
        alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    ) -> RewriteResult {
        self.rewrite_for_virtual_project_with_policy(
            source,
            source_type,
            roots,
            source_dir,
            false,
            alias_rewrite_policy,
        )
    }

    pub(crate) fn rewrite_for_package_shadow_with_alias_policy(
        &self,
        source: &str,
        source_type: SourceType,
        roots: (&Path, &Path),
        source_dir: Option<&Path>,
        alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    ) -> RewriteResult {
        self.rewrite_for_virtual_project_with_policy(
            source,
            source_type,
            roots,
            source_dir,
            true,
            alias_rewrite_policy,
        )
    }

    fn rewrite_for_virtual_project_with_policy(
        &self,
        source: &str,
        source_type: SourceType,
        roots: (&Path, &Path),
        source_dir: Option<&Path>,
        preserve_relative_declarations: bool,
        alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    ) -> RewriteResult {
        let project_root = roots.0.to_string_lossy();
        let dts_candidate = source_dir.is_some() && source_may_contain_relative_specifier(source);
        if !source.contains(".vue") && !source.contains(project_root.as_ref()) && !dts_candidate {
            return RewriteResult {
                code: source.to_compact_string(),
                source_map: ImportSourceMap::empty(),
            };
        }

        self.rewrite_with(source, source_type, |path, _| {
            self.rewrite_virtual_project_specifier(
                path,
                roots,
                source_dir,
                preserve_relative_declarations,
                alias_rewrite_policy,
            )
        })
    }

    fn rewrite_virtual_project_specifier(
        &self,
        path: &str,
        roots: (&Path, &Path),
        source_dir: Option<&Path>,
        preserve_relative_declarations: bool,
        alias_rewrite_policy: Option<&VirtualAliasRewritePolicy>,
    ) -> Option<vize_carton::String> {
        if let Some(rewritten) =
            authored_vue_ts::rewrite_authored_or_missing_vue_import(path, source_dir, true, false)
        {
            return Some(rewritten);
        }
        if let Some(source_dir) = source_dir
            && let Some(rewritten) = (!preserve_relative_declarations)
                .then(|| rewrite_relative_dts_specifier(path, source_dir, roots.0))
                .flatten()
                .or_else(|| rewrite_relative_vue_specifier(path, source_dir))
        {
            return Some(rewritten);
        }
        let candidate = Path::new(path);
        let canonical_candidate = vize_carton::path::canonicalize_non_verbatim(candidate);
        let canonical_project_root = vize_carton::path::canonicalize_non_verbatim(roots.0);
        if candidate.is_absolute()
            && let Ok(relative) = canonical_candidate
                .strip_prefix(canonical_project_root.as_path())
                .or_else(|_| candidate.strip_prefix(roots.0))
            && is_rewritable_project_specifier(relative)
        {
            if !path.ends_with(".vue") && !absolute_import_needs_virtual_rewrite(candidate) {
                return None;
            }
            let mut rewritten = cstr!("{}", roots.1.join(relative).display());
            if path.ends_with(".vue") {
                rewritten.push_str(".ts");
            }
            return Some(rewritten);
        }
        if is_rewritable_vue_specifier(path)
            && alias_rewrite_policy.is_none_or(|policy| policy.should_rewrite_vue_specifier(path))
        {
            Some(cstr!("{path}.ts"))
        } else {
            None
        }
    }
}
