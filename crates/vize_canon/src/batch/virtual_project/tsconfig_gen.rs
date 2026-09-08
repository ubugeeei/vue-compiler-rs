mod base_url;
pub(super) mod compiler_options;
mod compiler_options_snapshot;
pub use compiler_options_snapshot::snapshot_tsconfig_compiler_options;
mod control_alias;
mod native_options;
mod path_rebase;
pub(super) mod references;
pub use references::{TsconfigOwnershipCache, TsconfigOwnershipOptions, TsconfigSourceKind};
mod remap;
mod vue_alias;
mod vue_compiler_options;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use vize_carton::{String as CompactString, ToCompactString, cstr};

use crate::batch::error::CorsaResult;
use crate::batch::materialize_fs::write_if_changed;
use crate::batch::source_policy::SourceFilePolicy;

use super::{SHARED_HELPERS_FILE, VirtualProject};
use native_options::normalize_native_removed_options;

const PATH_SENSITIVE_COMPILER_OPTIONS: &[&str] = &[
    "baseUrl",
    "paths",
    "rootDir",
    "rootDirs",
    "outDir",
    "declarationDir",
    "typeRoots",
    "tsBuildInfoFile",
];

impl VirtualProject {
    pub(crate) fn tsconfig_preserves_unused_diagnostics(&self) -> bool {
        self.preserve_unused_diagnostics
    }

    /// Alias prefixes declared in the effective tsconfig `paths` map, with wildcard
    /// suffixes stripped. Used as a shard-planning cost model; aliases whose every
    /// target lives under `node_modules` are dependency cost and are skipped.
    pub(crate) fn path_alias_prefixes(&self) -> Vec<CompactString> {
        let Ok(compiler_options) =
            self.load_compiler_options(self.resolved_tsconfig_path().as_deref())
        else {
            return Vec::new();
        };
        let Some(paths) = compiler_options.get("paths").and_then(Value::as_object) else {
            return Vec::new();
        };
        paths
            .iter()
            .filter(|(_, targets)| {
                !targets.as_array().is_some_and(|targets| {
                    !targets.is_empty()
                        && targets.iter().all(|target| {
                            target
                                .as_str()
                                .is_some_and(|target| target.contains("node_modules"))
                        })
                })
            })
            .map(|(alias, _)| alias.trim_end_matches('*').to_compact_string())
            .collect()
    }

    pub(super) fn resolve_tsconfig_preserves_unused_diagnostics(&self) -> bool {
        let Some(tsconfig_path) = self.resolved_tsconfig_path() else {
            return false;
        };
        let Ok(compiler_options) = self.load_compiler_options(Some(tsconfig_path.as_path())) else {
            return false;
        };

        compiler_option_enabled(&compiler_options, "noUnusedLocals")
            || compiler_option_enabled(&compiler_options, "noUnusedParameters")
    }

    pub(super) fn write_tsconfig_file(
        &self,
        path: &Path,
        out_dir: Option<&Path>,
        declaration_map: bool,
    ) -> CorsaResult<()> {
        self.write_tsconfig_file_with_includes(path, out_dir, declaration_map, None)
    }

    /// Write a tsconfig whose `include` lists only the given virtual paths
    /// (plus the shared stub files). Used for shard configs that partition the
    /// project across parallel Corsa CLI runs.
    pub(crate) fn write_shard_tsconfig(
        &self,
        shard_index: usize,
        include_virtual_paths: &[&Path],
    ) -> CorsaResult<PathBuf> {
        let config_path = self
            .virtual_root
            .join(cstr!("tsconfig.shard{shard_index}.json").as_str());
        self.write_tsconfig_file_with_includes(
            &config_path,
            None,
            false,
            Some(include_virtual_paths),
        )?;
        Ok(config_path)
    }

    pub(super) fn write_tsconfig_file_with_includes(
        &self,
        path: &Path,
        out_dir: Option<&Path>,
        declaration_map: bool,
        include_virtual_paths: Option<&[&Path]>,
    ) -> CorsaResult<()> {
        let tsconfig =
            self.generate_tsconfig_value(out_dir, declaration_map, include_virtual_paths)?;
        let content = serde_json::to_string_pretty(&tsconfig)?;
        write_if_changed(path, content.as_bytes())?;
        Ok(())
    }

    fn generate_tsconfig_value(
        &self,
        out_dir: Option<&Path>,
        declaration_map: bool,
        include_virtual_paths: Option<&[&Path]>,
    ) -> CorsaResult<Value> {
        let mut config = Map::new();
        let original_tsconfig = self.resolved_tsconfig_path();

        // Flatten the effective compiler options instead of `extends`-ing the
        // user's tsconfig, so Corsa does not re-parse the source chain or inherit
        // real-tree `files`/`include` entries into the virtual program.
        let flattened = self.load_compiler_options_flattened(original_tsconfig.as_deref())?;
        let mut compiler_options = flattened.options;

        // Capture the original path-alias map and type roots before stripping
        // path-sensitive options, so they can be re-anchored into the virtual
        // mirror below. A declared `baseUrl` becomes a synthesized `"*"` alias
        // so bare specifiers keep resolving after the option itself is
        // stripped (#3886).
        let mut original_paths = compiler_options
            .get("paths")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        base_url::insert_wildcard_alias(&mut original_paths, flattened.base_url.as_deref());
        let original_type_roots = compiler_options
            .get("typeRoots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let original_root_dir = compiler_options
            .get("rootDir")
            .and_then(Value::as_str)
            .map(|root_dir| root_dir.to_compact_string());

        for option in PATH_SENSITIVE_COMPILER_OPTIONS {
            compiler_options.remove(*option);
        }
        // The mirror owns only virtual/support files; `composite` would make
        // normal imports look like project-build membership errors (TS6307).
        compiler_options.remove("composite");
        normalize_native_removed_options(&mut compiler_options);
        compiler_options.insert("allowImportingTsExtensions".into(), Value::Bool(true));
        if self
            .package_routes
            .values()
            .any(|binding| binding.route.is_some())
        {
            compiler_options.insert("allowArbitraryExtensions".into(), Value::Bool(true));
        }
        if self.needs_vue_jsx_compiler_options() {
            compiler_options
                .entry("jsx")
                .or_insert_with(|| Value::String("preserve".into()));
            compiler_options
                .entry("jsxImportSource")
                .or_insert_with(|| Value::String("vue".into()));
        }

        // Re-anchor tsconfig `paths` into the virtual mirror. Without this the
        // aliases inherited via `extends` resolve against the real source tree,
        // where `.vue` files only match the ambient `*.vue` stub (default export
        // only) and named re-exports surface as false `TS2614`. Each alias
        // target gets a mirror candidate first (so the generated `.vue.ts`
        // modules win) and the real-tree path as a fallback (so aliases to files
        // outside the checked set keep resolving).
        let remapped_paths = if original_paths.is_empty() {
            Map::new()
        } else {
            self.remap_paths(&original_paths)
        };
        if !remapped_paths.is_empty() {
            compiler_options.insert("paths".into(), Value::Object(remapped_paths));
        }

        // Re-anchor custom `typeRoots` the same way: list the mirror copy and
        // the real-tree directory. TypeScript scans every listed root and
        // skips missing directories, so `types: [...]` entries served by a
        // custom root keep resolving instead of raising a false TS2688 that
        // only exists inside the mirror.
        if !original_type_roots.is_empty() {
            compiler_options.insert(
                "typeRoots".into(),
                Value::Array(self.remap_dir_entries(&original_type_roots)),
            );
        }

        if let Some(out_dir) = out_dir {
            compiler_options.insert("noEmit".into(), Value::Bool(false));
            compiler_options.insert("declaration".into(), Value::Bool(true));
            compiler_options.insert("emitDeclarationOnly".into(), Value::Bool(true));
            compiler_options.insert("declarationMap".into(), Value::Bool(declaration_map));
            // Honor a configured `rootDir` (see [`path_rebase`]) and fall back
            // to inference when nothing is configured.
            let desired_root_dir = path_rebase::root_dir_into_mirror(
                &self.project_root,
                &self.virtual_root,
                original_root_dir.as_deref(),
            )
            .unwrap_or_else(|| self.common_virtual_source_dir());
            // A workspace-package source is part of the declaration program so
            // its public type can flow into the caller's output, but it is not
            // one of the caller-selected declaration roots. TypeScript still
            // applies TS6059 to imported files, so temporarily widen rootDir
            // when such an inferred source sits outside the configured root.
            // The output finalizer restores the configured layout and removes
            // declarations for those inferred dependencies after emit.
            let has_inferred_source_outside_root = self.virtual_files.values().any(|file| {
                let original = vize_carton::path::canonicalize_non_verbatim(&file.original_path);
                !self.is_declaration_root(&original)
                    && !file.virtual_path.starts_with(&desired_root_dir)
            });
            let root_dir = if has_inferred_source_outside_root {
                self.common_virtual_source_dir()
            } else {
                desired_root_dir
            };
            compiler_options.insert(
                "rootDir".into(),
                Value::String(root_dir.to_string_lossy().into_owned()),
            );
            compiler_options.insert(
                "outDir".into(),
                Value::String(out_dir.to_string_lossy().into_owned()),
            );
        } else {
            compiler_options.remove("declaration");
            compiler_options.remove("emitDeclarationOnly");
            compiler_options.remove("declarationMap");
            compiler_options.remove("outDir");
            compiler_options.insert("noEmit".into(), Value::Bool(true));
        }

        let include_js =
            SourceFilePolicy::from_compiler_options(&compiler_options).allows_javascript();
        config.insert("compilerOptions".into(), Value::Object(compiler_options));
        config.insert(
            "include".into(),
            Value::Array(
                self.include_paths(include_virtual_paths, include_js)
                    .into_iter()
                    .map(|path| Value::String(path.into()))
                    .collect(),
            ),
        );
        config.insert("exclude".into(), Value::Array(Vec::new()));

        Ok(Value::Object(config))
    }

    pub(super) fn configured_declaration_root_dir(&self) -> CorsaResult<Option<PathBuf>> {
        let original_tsconfig = self.resolved_tsconfig_path();
        let flattened = self.load_compiler_options_flattened(original_tsconfig.as_deref())?;
        Ok(path_rebase::root_dir_into_mirror(
            &self.project_root,
            &self.virtual_root,
            flattened.options.get("rootDir").and_then(Value::as_str),
        ))
    }

    pub(super) fn needs_vue_jsx_compiler_options(&self) -> bool {
        self.virtual_files.values().any(|file| {
            file.virtual_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.ends_with(".vue.tsx")
                        || name.ends_with(".tsx.ts")
                        || name.ends_with(".jsx.ts")
                })
        })
    }

    pub(super) fn include_paths(
        &self,
        paths: Option<&[&Path]>,
        include_js: bool,
    ) -> Vec<CompactString> {
        let relative = |path: &Path| {
            path.strip_prefix(&self.virtual_root)
                .ok()
                .map(|path| path.to_string_lossy().to_compact_string())
        };
        let mut includes: Vec<_> = match paths {
            Some(paths) => paths.iter().filter_map(|path| relative(path)).collect(),
            None => self
                .virtual_files
                .values()
                .filter(|file| {
                    let original =
                        vize_carton::path::canonicalize_non_verbatim(&file.original_path);
                    self.is_declaration_root(&original)
                })
                .filter_map(|file| relative(&file.virtual_path))
                .collect(),
        };
        if paths.is_none() {
            includes.extend(self.package_shadow_files.iter().filter_map(
                |(materialized_path, canonical_path)| {
                    let file = self.virtual_files.get(canonical_path)?;
                    let original =
                        vize_carton::path::canonicalize_non_verbatim(&file.original_path);
                    self.is_declaration_root(&original)
                        .then(|| relative(materialized_path))
                        .flatten()
                },
            ));
        }
        if include_js {
            includes.extend(self.javascript_passthrough_files().filter_map(relative));
        }
        self.push_stub_include_paths(&mut includes);
        if self.uses_shared_helpers() {
            includes.push(SHARED_HELPERS_FILE.into());
        }
        includes.sort();
        includes.dedup();
        includes
    }
}

#[allow(clippy::disallowed_types)]
fn compiler_option_enabled(options: &Map<std::string::String, Value>, name: &str) -> bool {
    options.get(name).and_then(Value::as_bool).unwrap_or(false)
}
