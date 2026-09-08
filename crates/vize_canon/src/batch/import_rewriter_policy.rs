//! Import-specifier rewrite policy shared by batch and editor paths.

use std::path::Path;

use vize_carton::{String, cstr};

use super::{
    ImportRewriter, authored_vue_ts::rewrite_authored_or_missing_vue_import,
    virtual_rewrite::is_rewritable_vue_specifier,
};

impl ImportRewriter {
    pub(crate) fn rewrite_module_specifier_with_missing_vue_policy(
        &self,
        path: &str,
        source_dir: Option<&Path>,
        preserve_missing_vue_diagnostics: bool,
        include_absolute_missing_vue: bool,
    ) -> Option<String> {
        if let Some(rewritten) = rewrite_authored_or_missing_vue_import(
            path,
            source_dir,
            preserve_missing_vue_diagnostics,
            include_absolute_missing_vue,
        ) {
            return Some(rewritten);
        }
        if is_rewritable_vue_specifier(path) {
            Some(cstr!("{path}.ts"))
        } else {
            None
        }
    }
}
