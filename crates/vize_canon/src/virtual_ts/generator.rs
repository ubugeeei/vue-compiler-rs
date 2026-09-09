mod anchors;
mod auto_import_stubs;
mod component_constructors;
mod component_export;
mod css_modules;
mod emits;
mod entry;
mod fallthrough;
pub(super) mod generics;
mod global_components;
mod imports;
mod legacy_vue2;
mod macro_anchors;
mod options_api;
mod options_api_bridge;
mod options_api_props_identifiers;
mod options_api_support;
mod script_blocks;
mod script_module;
mod setup_helpers;
mod setup_props;
pub(super) mod setup_scope;
mod setup_type_exports;
mod spans;
mod template_refs;
mod type_only_imports;
mod unresolved_components;
use self::anchors::emit_setup_binding_anchors;
use self::auto_import_stubs::emit_auto_import_stubs;
use self::component_constructors::{ComponentInstanceAliases, emit_component_constructors};
use self::component_export::{emit_authored_component_aliases, emit_default_export_declaration};
use self::css_modules::CssModuleAssertions;
use self::emits::{emit_emit_props_helper, emit_emits_type, emit_exposed_type, emit_slots_type};
pub use self::entry::{
    generate_virtual_ts, generate_virtual_ts_with_offsets,
    generate_virtual_ts_with_offsets_options_api,
};
use self::generics::{HoistedGenericAliases, generic_injection_point, references_any_identifier};
use self::global_components::GlobalComponentPlan;
use self::imports::{
    collect_imported_names, emit_reference_path_directives, emit_reference_type_directives,
};
pub use self::legacy_vue2::generate_virtual_ts_with_offsets_legacy_vue2;
use self::macro_anchors::emit_setup_scope_macro_anchors;
use self::options_api::{find_default_export_targets, generate_options_api_variables};
use self::options_api_bridge::generate_options_api_bridge;
use self::options_api_props_identifiers::PropsConstAssertions;
use self::options_api_support::find_options_api_props;
use self::script_blocks::ScriptBlockScopes;
use self::setup_helpers::{SetupHelperComponentContext, emit_setup_helpers};
use self::setup_props::{generate_setup_props, prop_source};
use self::setup_type_exports::SetupTypeExportsPlan;
use self::spans::{DEFINE_COMPONENT_REF, rewrite_export_default_for_module_scope, template_usage};
use self::type_only_imports::{
    collect_syntactic_type_only_imported_names, should_collect_syntactic_type_only_imported_names,
};
use self::unresolved_components::emit_unresolved_components;
use super::{
    helpers::{SETUP_SCOPE_HELPER_NAMES, generate_template_context},
    import_meta::emit_import_meta_augmentation,
    macro_type_mappings::MacroTypeMappings,
    props::{
        OptionsApiPropsSource, add_generic_defaults, collect_template_prop_names,
        extract_generic_names, strip_const_modifiers,
    },
    scope::{ScopeGenerationOptions, emit_slot_payload_helpers, generate_scope_closures},
    types::{
        DEFAULT_LIB_REFERENCES, VirtualTsGenerationOptions, VirtualTsOptions, VirtualTsOutput,
        VizeMapping, emit_lib_reference_directives,
    },
};
use vize_carton::{FxHashMap, FxHashSet, String, append, config::VueVersion, cstr, profile};
use vize_croquis::{Croquis, ScopeData, ScopeKind};

pub(crate) fn generate_virtual_ts_with_offsets_and_checks(
    summary: &Croquis,
    script_content: Option<&str>,
    template_ast: Option<&vize_relief::RootNode<'_>>,
    script_offset: u32,
    template_offset: u32,
    options: &VirtualTsOptions,
    generation_options: VirtualTsGenerationOptions<'_>,
) -> VirtualTsOutput {
    let check_options = generation_options.check_options;
    let check_props = check_options.check_props;
    let script_source_offset =
        |offset| generation_options.script_source_offset(script_offset, offset);
    // Configured Vue dialect drives the Vue 2/3 template instance shape.
    let dialect = generation_options.dialect;
    let legacy_vue2 =
        generation_options.legacy_vue2 || matches!(dialect, VueVersion::V2 | VueVersion::V2_7);
    let _ = generation_options.template_syntax_quirks;
    let options_api = generation_options.options_api || legacy_vue2;
    let hoist_shared_preamble = generation_options.hoist_shared_preamble;
    let mut ts = String::default();
    let mut mappings: Vec<VizeMapping> = Vec::new();
    let mut semantic_links = Vec::new();
    let preserve_unused_diagnostics = generation_options.preserve_unused_diagnostics;
    let (template_usage_names, has_template_scope) =
        template_usage(summary, template_ast, generation_options);
    let template_referenced_names = preserve_unused_diagnostics.then_some(&template_usage_names);
    let reference_setup_bindings_comment =
        self::anchors::setup_binding_anchor_comment(preserve_unused_diagnostics);
    let lib_references = generation_options
        .lib_references
        .unwrap_or(DEFAULT_LIB_REFERENCES);
    emit_lib_reference_directives(&mut ts, lib_references);
    emit_reference_path_directives(&mut ts, &options.reference_paths);
    let has_script_reference_types = emit_reference_type_directives(&mut ts, script_content);
    ts.push_str("// ============================================\n// Virtual TypeScript for Vue SFC Type Checking\n// Generated by vize\n// ============================================\n\n");

    // Check for generic type parameter from <script setup generic="T">
    let (generic_param, is_async) = summary
        .scopes
        .iter()
        .find(|s| matches!(s.kind, ScopeKind::ScriptSetup))
        .map(|s| {
            if let ScopeData::ScriptSetup(data) = s.data() {
                (data.generic.as_ref().map(|s| s.as_str()), data.is_async)
            } else {
                (None, false)
            }
        })
        .unwrap_or((None, false));

    if hoist_shared_preamble {
        // ImportMeta augmentation and shared type helpers live once per
        // program in the ambient helpers file (SHARED_PREAMBLE_DTS); the
        // module no longer augments global scope itself.
        ts.push_str("// Shared preamble hoisted to the program-wide __vize_helpers.d.ts\n");
    } else {
        // ImportMeta augmentation (must be at top level, before any code)
        emit_import_meta_augmentation(&mut ts, !generation_options.omit_vite_client_reference);
        ts.push('\n');
    }

    // Module scope: Extract imports, re-exports, and type declarations to module level.
    // Type declarations (interface, type, enum) must be at module level so they
    // are accessible from `export type Props = ...` outside __setup().
    ts.push_str("// ========== Module Scope (imports) ==========\n");
    if !hoist_shared_preamble {
        ts.push_str(legacy_vue2::vue_type_helpers(legacy_vue2, dialect));
    }
    emit_slot_payload_helpers(&mut ts, summary, !hoist_shared_preamble);

    let has_script_setup = summary
        .scopes
        .iter()
        .any(|scope| matches!(scope.kind, ScopeKind::ScriptSetup));
    let has_plain_script_scope = summary
        .scopes
        .iter()
        .any(|scope| matches!(scope.kind, ScopeKind::NonScriptSetup))
        || (script_content.is_some() && !has_script_setup);
    let mut named_value_exports = self::script_module::collect_normal_script_named_value_exports(
        script_content,
        has_script_setup,
        has_plain_script_scope,
    );
    // A `namespace` cannot stay inside `__setup()` (TS1235), so it and any
    // declaration it merges with move to module scope instead of through the
    // export bridge.
    let namespace_hoist = self::script_module::NamespaceHoistPlan::collect(
        script_content,
        has_script_setup,
        has_plain_script_scope,
    );
    namespace_hoist.reconcile_exports(&mut named_value_exports);
    let script_blocks = ScriptBlockScopes::collect(summary, script_content, has_script_setup);
    let setup_type_exports = SetupTypeExportsPlan::new(summary, script_content, &script_blocks);

    // Classify the main `<script>` default export in one parse. A plain
    // `export default { ... }` (Options API shape) gets wrapped with Vue's
    // `defineComponent` so `this` inside computed/methods is typed by Vue's
    // options machinery; a class default export (class-component shape) keeps
    // its decorators on a standalone class declaration plus a
    // `const __default__ =` alias (a bare `const __default__ = class {}`
    // rewrite would move the decorators onto a class expression — TS1206).
    // Script setup virtual TS is never touched: in setup-only SFCs no rewrite
    // target exists, so the lookup is skipped entirely.
    let default_export_targets = if !has_script_setup || has_plain_script_scope {
        profile!(
            "canon.virtual_ts.find_default_export_targets",
            script_content
                .map(find_default_export_targets)
                .unwrap_or_default()
        )
    } else {
        Default::default()
    };
    let default_export_object = default_export_targets.object;
    let default_export_class = default_export_targets.class;
    let default_export_expr = default_export_targets.expr;
    if default_export_object.is_some() {
        ts.push_str(legacy_vue2::define_component_helper(legacy_vue2, dialect));
    }
    // Collect all module-level statement spans from croquis analysis once and
    // keep them sorted. Later script-body emission advances an index through
    // this list, so each source line checks only the overlapping tail instead
    // of rescanning imports/re-exports/type declarations from the start.
    let module_spans: Vec<(u32, u32)> = profile!("canon.virtual_ts.collect_module_spans", {
        let mut module_spans = Vec::new();
        for imp in &summary.import_statements {
            module_spans.push((imp.start, imp.end));
        }
        if let Some(script) = script_content {
            module_spans.extend(self::script_module::collect_line_module_spans(script));
        }
        module_spans.extend(namespace_hoist.spans().iter().copied());
        for re in &summary.re_exports {
            module_spans.push((re.start, re.end));
        }
        script_blocks.module_spans(summary, script_content, module_spans)
    });

    // Re-declare SFC generics on hoisted declarations with safe defaults.
    let generic_injection: Option<(String, Vec<String>)> = generic_param.map(|g| {
        // Type aliases/interfaces cannot retain function-only `const` modifiers (TS1277).
        let defaults = strip_const_modifiers(&add_generic_defaults(g));
        let names = extract_generic_names(g)
            .split(',')
            .map(|n| String::from(n.trim()))
            .filter(|n| !n.is_empty())
            .collect();
        (defaults, names)
    });
    let hoisted_type_spans: FxHashMap<(u32, u32), &str> = if generic_injection.is_some() {
        script_blocks.hoisted_type_spans(summary, script_content)
    } else {
        FxHashMap::default()
    };

    namespace_hoist.emit_ambient_captures(&mut ts, &named_value_exports);
    // Whether a default-export rewrite declared `__default__`, tracked as
    // state: grepping the generated text would match helper type positions
    // and user code that merely mentions the name (#3888).
    let mut declared_default_alias = false;
    if let Some(script) = script_content {
        profile!("canon.virtual_ts.emit_module_statements", {
            // Emit each module-level statement with source mapping
            for &(start, end) in &module_spans {
                let text = &script[start as usize..end as usize];

                // Splice the SFC generic parameters into a hoisted
                // type/interface declaration that references them, so the
                // reference resolves at module scope.
                if let Some((defaults, names)) = &generic_injection
                    && let Some(type_name) = hoisted_type_spans.get(&(start, end))
                    && references_any_identifier(text, names)
                    && let Some(inject_at) = generic_injection_point(text, type_name)
                {
                    let (prefix, suffix) = text.split_at(inject_at);
                    let src_base = script_source_offset(start as usize);

                    let gen_start = ts.len();
                    ts.push_str(prefix);
                    mappings.push(VizeMapping {
                        gen_range: gen_start..ts.len(),
                        src_range: src_base..(src_base + prefix.len()),
                        sub_spans: Vec::new(),
                    });

                    // Synthetic parameter list; no corresponding source span.
                    append!(ts, "<{defaults}>");
                    // Avoid forming `>=` when the alias has no space before `=`.
                    if suffix.starts_with('=') {
                        ts.push(' ');
                    }

                    let gen_start = ts.len();
                    ts.push_str(suffix);
                    ts.push('\n');
                    mappings.push(VizeMapping {
                        gen_range: gen_start..ts.len(),
                        src_range: (src_base + prefix.len())
                            ..(src_base + prefix.len() + suffix.len()),
                        sub_spans: Vec::new(),
                    });
                    continue;
                }

                let gen_start = ts.len();
                if text.contains("export default") {
                    // Re-base the script-absolute default-export offsets onto
                    // this module span so the rewrite can wrap a plain options
                    // object with `defineComponent`, or rewrite any other shape
                    // to a bare `const __default__ = <expr>`, by slicing on AST
                    // byte offsets instead of scanning lines.
                    let span_start = start as usize;
                    let span_end = end as usize;
                    let rebase = |(export_start, inner_start, inner_end): (usize, usize, usize)| {
                        (export_start >= span_start && inner_end <= span_end).then_some((
                            export_start - span_start,
                            inner_start - span_start,
                            inner_end - span_start,
                        ))
                    };
                    let span_relative_object = default_export_object.and_then(rebase);
                    let span_relative_expr = default_export_expr.and_then(rebase);
                    let rewritten = rewrite_export_default_for_module_scope(
                        text,
                        span_relative_object,
                        span_relative_expr,
                    );
                    // Every rewrite shape declares the alias; a span with no
                    // rewriteable default export comes back untouched.
                    declared_default_alias |= rewritten.as_str() != text;
                    ts.push_str(&rewritten);
                    ts.push_str("void __default__;\n");
                } else {
                    ts.push_str(text);
                }
                ts.push('\n');
                let gen_end = ts.len();
                mappings.push(VizeMapping {
                    gen_range: gen_start..gen_end,
                    src_range: script_source_offset(start as usize)
                        ..script_source_offset(end as usize),
                    sub_spans: Vec::new(),
                });
            }

            // Void-reference imported names that match setup-scope helper names.
            // These get shadowed by __setup() declarations, causing TS6133 at module level.
            let shadowed_imports: Vec<&&str> = SETUP_SCOPE_HELPER_NAMES
                .iter()
                .filter(|&&name| summary.bindings.bindings.contains_key(name))
                .collect();
            if !shadowed_imports.is_empty() {
                ts.push_str(
                    "// Prevent TS6133 for imports shadowed by setup-scope compiler macros\n",
                );
                for name in &shadowed_imports {
                    append!(ts, "void {name};\n");
                }
            }
        });
    }
    let hoisted_generic_aliases =
        HoistedGenericAliases::collect(summary, script_content, generic_param);
    if let Some(aliases) = &hoisted_generic_aliases {
        aliases.emit_module_aliases(&mut ts);
    }

    let global_components =
        GlobalComponentPlan::new(summary, legacy_vue2, has_script_reference_types);
    // The template-scope unwrap set also needs it: an auto-import already
    // imported by a plain `<script>` must not gain a second shadow.
    let needs_imported_names = !options.auto_import_stubs.is_empty()
        || !options.auto_import_bindings.is_empty()
        || (global_components.enabled() && !summary.component_usages.is_empty());
    let imported_names: FxHashSet<&str> = if needs_imported_names {
        profile!(
            "canon.virtual_ts.extract_imported_names",
            collect_imported_names(summary, script_content)
        )
    } else {
        FxHashSet::default()
    };
    let syntactic_type_only_imported_names =
        if should_collect_syntactic_type_only_imported_names(summary, &global_components) {
            profile!(
                "canon.virtual_ts.extract_syntactic_type_only_imported_names",
                collect_syntactic_type_only_imported_names(summary, script_content)
            )
        } else {
            FxHashSet::default()
        };
    if !options.auto_import_stubs.is_empty() {
        profile!(
            "canon.virtual_ts.emit_auto_import_stubs",
            emit_auto_import_stubs(
                &mut ts,
                summary,
                options,
                &imported_names,
                &syntactic_type_only_imported_names,
            )
        );
    }
    global_components.emit(
        &mut ts,
        summary,
        options,
        &imported_names,
        &syntactic_type_only_imported_names,
    );
    ts.push('\n');

    // Derive a real cross-file `Props` type from macro or Options API input.
    let options_api_props: Option<OptionsApiPropsSource> =
        if options_api && summary.macros.props().is_empty() {
            script_content.and_then(find_options_api_props)
        } else {
            None
        };
    let source_offset = &script_source_offset;
    let setup_props_plan = generate_setup_props(
        &mut ts,
        prop_source(&mut mappings, summary, script_content, source_offset),
        generic_param,
        options_api_props.as_ref(),
        &syntactic_type_only_imported_names,
        setup_type_exports.exports_public_type("Props"),
    );
    ts.push_str("// ========== Setup Scope ==========\n");
    let async_prefix = if is_async { "async " } else { "" };
    let generic_params = generic_param.map(|g| cstr!("<{g}>")).unwrap_or_default();
    append!(ts, "{async_prefix}function __setup{generic_params}() {{\n",);
    if let Some(aliases) = &hoisted_generic_aliases {
        aliases.emit_setup_aliases(&mut ts);
    }

    emit_setup_helpers(
        &mut ts,
        SetupHelperComponentContext {
            summary,
            options,
            syntactic_type_only_imported_names: &syntactic_type_only_imported_names,
        },
        script_content,
        generic_param,
        hoist_shared_preamble,
        template_ast,
    );
    ts.push_str("\n\n");

    if let Some(script) = script_content {
        profile!("canon.virtual_ts.emit_script_body", {
            ts.push_str("  // User setup code\n");
            let script_gen_start = ts.len();
            // `split('\n')` preserves byte offsets for CRLF; `lines()` strips `\r`.
            let mut src_byte_offset: usize = 0; // offset within script content
            let mut module_span_index = 0usize;
            let named_value_export_starts =
                self::script_module::collect_named_value_export_starts(script);
            let mut props_const_assertions = PropsConstAssertions::new(script, options_api);
            let mut css_module_assertions = CssModuleAssertions::new(script, options);
            let mut pending_wrap_close: Option<usize> = None;
            // Deferred class-component alias: `(class_end, name)`.
            let mut pending_class_alias: Option<(usize, &str)> = None;
            let mut emitted_default_alias = false;
            let uses_import_meta = self::script_module::emit_import_meta_polyfill(&mut ts, script);

            for raw_line in script.split('\n') {
                let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
                let raw_byte_len = raw_line.len() + 1;

                let line_start = src_byte_offset;
                let line_end = line_start + raw_line.len(); // use raw length for span check
                let source_token_start = line_start + line.len() - line.trim_start().len();
                while module_span_index < module_spans.len()
                    && module_spans[module_span_index].1 as usize <= line_start
                {
                    module_span_index += 1;
                }
                let is_module_level = module_spans[module_span_index..]
                    .iter()
                    .take_while(|&&(start, _)| (start as usize) < line_end)
                    .any(|&(start, end)| line_start < end as usize && line_end > start as usize);
                if is_module_level {
                    src_byte_offset += raw_byte_len;
                    continue;
                }
                ts.push_str("  "); // indentation (not in source)
                let gen_content_start = ts.len();

                let mut output_line = std::borrow::Cow::Borrowed(line);
                setup_type_exports.strip_modifiers(&mut output_line, line_start as u32);

                // Close the `defineComponent(` wrap right after the wrapped
                // options object's closing brace.
                if let Some(close_offset) = pending_wrap_close
                    && close_offset > line_start
                    && close_offset <= line_end
                    && close_offset - line_start <= line.len()
                    && line.is_char_boundary(close_offset - line_start)
                {
                    let column = close_offset - line_start;
                    #[allow(clippy::disallowed_types)]
                    {
                        output_line = std::borrow::Cow::Owned(
                            cstr!("{}){}", &line[..column], &line[column..]).into(),
                        );
                    }
                    pending_wrap_close = None;
                }

                css_module_assertions.splice_output_line(&mut output_line, line_start);
                props_const_assertions.splice_output_line(&mut output_line, line_start);

                // Strip `export` from non-import lines inside setup scope
                let trimmed_line = output_line.trim_start();
                if let Some(default_expr) = trimmed_line
                    .strip_prefix("export default")
                    .filter(|rest| options_api_support::follows_default_keyword(rest))
                {
                    emitted_default_alias = true;
                    declared_default_alias = true;
                    let leading_ws = &output_line[..output_line.len() - trimmed_line.len()];
                    // A class default export (the class-component shape) stays
                    // a real class declaration so `@Component()` decorators
                    // remain valid (a bare `const __default__ = class {}`
                    // rewrite moves them onto a class expression — TS1206). The
                    // `const __default__ = <Name>` alias is deferred until the
                    // class body closes.
                    let class_default =
                        if has_script_setup || matches!(output_line, std::borrow::Cow::Owned(_)) {
                            None
                        } else {
                            default_export_class.filter(
                                |&(export_start, class_start, class_end, _, _)| {
                                    export_start >= line_start
                                        && class_start >= line_start
                                        && class_start < line_end
                                        && class_end > line_start
                                        && class_start - line_start <= line.len()
                                        && line.is_char_boundary(class_start - line_start)
                                },
                            )
                        };
                    // Wrap a plain object-literal default export (Options
                    // API) with `defineComponent` so `this` in
                    // computed/methods gets Vue's instance typing. Applies
                    // only to the plain <script> block; any other shape keeps
                    // the bare `const __default__ =` rewrite.
                    let wrap_object = if class_default.is_some()
                        || has_script_setup
                        || matches!(output_line, std::borrow::Cow::Owned(_))
                    {
                        None
                    } else {
                        default_export_object.filter(|&(export_start, object_start, _)| {
                            export_start >= line_start
                                && object_start >= line_start
                                && object_start < line_end
                                && object_start - line_start <= line.len()
                                && line[object_start - line_start..].starts_with('{')
                        })
                    };
                    #[allow(clippy::disallowed_types)]
                    if let Some((class_start, class_end, name_start, name_end)) =
                        class_default.map(|(_, cs, ce, ns, ne)| (cs, ce, ns, ne))
                    {
                        // Drop the `export default ` keyword, keep the class
                        // (and any same-line trailing decorator) verbatim.
                        let class_column = class_start - line_start;
                        let name = &script[name_start..name_end];
                        output_line = std::borrow::Cow::Owned(
                            cstr!("{leading_ws}{}", &line[class_column..]).into(),
                        );
                        pending_class_alias = Some((class_end, name));
                    } else if let Some((object_column, object_end)) =
                        wrap_object.and_then(|(_, object_start, object_end)| {
                            let object_column = object_start - line_start;
                            (line.len() - default_expr.len() <= object_column)
                                .then_some((object_column, object_end))
                        })
                    {
                        let keyword_end = line.len() - default_expr.len();
                        if object_end <= line_end && object_end - line_start <= line.len() {
                            // Single-line `export default { ... }`.
                            let close_column = object_end - line_start;
                            output_line = std::borrow::Cow::Owned(
                                cstr!(
                                    "{leading_ws}const __default__ ={}{DEFINE_COMPONENT_REF}({}){}",
                                    &line[keyword_end..object_column],
                                    &line[object_column..close_column],
                                    &line[close_column..],
                                )
                                .into(),
                            );
                        } else {
                            pending_wrap_close = Some(object_end);
                            output_line = std::borrow::Cow::Owned(
                                cstr!(
                                    "{leading_ws}const __default__ ={}{DEFINE_COMPONENT_REF}({}",
                                    &line[keyword_end..object_column],
                                    &line[object_column..],
                                )
                                .into(),
                            );
                        }
                    } else {
                        output_line = std::borrow::Cow::Owned(
                            cstr!("{leading_ws}const __default__ ={}", default_expr).into(),
                        );
                    }
                } else if named_value_export_starts.contains(&(source_token_start as u32))
                    && trimmed_line.starts_with("export ")
                    && !trimmed_line.starts_with("export type ")
                    && !trimmed_line.starts_with("export interface ")
                {
                    let leading_ws = &output_line[..output_line.len() - trimmed_line.len()];
                    if let Some(rest) = trimmed_line.strip_prefix("export ") {
                        #[allow(clippy::disallowed_types)]
                        {
                            output_line =
                                std::borrow::Cow::Owned(cstr!("{leading_ws}{rest}").into());
                        }
                    }
                }
                // Replace import.meta with polyfill variable to avoid TS1343
                if uses_import_meta && output_line.contains("import.meta") {
                    #[allow(clippy::disallowed_types)]
                    {
                        output_line = std::borrow::Cow::Owned(
                            output_line.replace("import.meta", "__import_meta"),
                        );
                    }
                }

                ts.push_str(&output_line);
                let gen_content_end = ts.len();
                ts.push('\n');
                // Map the line content (excluding the "  " indent prefix)
                if !line.is_empty() {
                    let src_line_start = script_source_offset(src_byte_offset);
                    let src_line_end = src_line_start + line.len();
                    mappings.push(VizeMapping {
                        gen_range: gen_content_start..gen_content_end,
                        src_range: src_line_start..src_line_end,
                        sub_spans: Vec::new(),
                    });
                }
                // Once the class body has closed, append the deferred
                // `const __default__ = <Name>` alias for the class component.
                if let Some((class_end, name)) = pending_class_alias
                    && class_end <= line_end
                {
                    append!(ts, "  const __default__ = {name};\n");
                    pending_class_alias = None;
                }
                src_byte_offset += raw_byte_len;
            }
            if let Some((_, name)) = pending_class_alias.take() {
                // Defensive: the class body's closing brace was never seen.
                append!(ts, "  const __default__ = {name};\n");
            }
            if pending_wrap_close.take().is_some() {
                // Defensive: if the object close was never emitted, close the `defineComponent(`
                // so the generated module stays parseable.
                ts.push_str("  )\n");
            }
            if emitted_default_alias {
                ts.push_str("  void __default__;\n");
            }
            let script_gen_end = ts.len();
            append!(
                ts,
                "  // @vize-map: {script_gen_start}:{script_gen_end} -> 0:{}\n\n",
                script.len()
            );

            // Vue 2 only; see the bridge module doc for why Vue 3 skips it.
            if legacy_vue2 {
                let offset = script_offset as usize;
                generate_options_api_bridge(&mut ts, &mut mappings, summary, script, offset);
            }
        });
    }
    setup_props_plan.emit_artifact(
        &mut ts,
        prop_source(&mut mappings, summary, script_content, source_offset),
    );
    // Template scope (nested inside setup)
    if has_template_scope && check_options.check_template_bindings {
        profile!("canon.virtual_ts.emit_template_scope", {
            ts.push_str("  // ========== Template Scope (inherits from setup) ==========\n");

            let template_ref_unwraps = template_refs::collect_and_emit_scope_preamble(
                &mut ts,
                summary,
                options_api,
                &template_usage_names,
                script_content,
                &imported_names,
                options,
                generation_options,
                generic_param.is_some(),
                &mut semantic_links,
            );

            // Vue template context (available in template expressions)
            let template_context = profile!(
                "canon.virtual_ts.generate_template_context",
                generate_template_context(options, dialect, legacy_vue2)
            );
            ts.push_str(&template_context);
            ts.push('\n');
            let maps = &mut mappings;
            let src = prop_source(maps, summary, script_content, &script_source_offset);
            profile!("canon.virtual_ts.generate_props_variables", {
                setup_props_plan.generate_props_variables(&mut ts, src, check_props && !legacy_vue2)
            });
            if options_api {
                profile!(
                    "canon.virtual_ts.generate_options_api_variables",
                    generate_options_api_variables(&mut ts, summary, options, script_content)
                );
            }
            let template_prop_names = profile!(
                "canon.virtual_ts.collect_template_prop_names",
                collect_template_prop_names(summary)
            );
            if check_options.any_enabled() {
                profile!(
                    "canon.virtual_ts.generate_scope_closures",
                    generate_scope_closures(
                        &mut ts,
                        &mut mappings,
                        &mut semantic_links,
                        summary,
                        &template_prop_names,
                        template_offset,
                        ScopeGenerationOptions {
                            check_options,
                            virtual_ts_options: options,
                            setup_spread_bindings: template_ref_unwraps.setup_spread_bindings(),
                            syntactic_type_only_imported_names: &syntactic_type_only_imported_names,
                            template_ast,
                            check_unresolved_global_components: global_components.component_check(),
                            legacy_vue2,
                            options_api,
                            preserve_event_navigation: generation_options.preserve_event_navigation,
                            has_default_alias: declared_default_alias,
                            script_content,
                        },
                    )
                );
            }
            emit_unresolved_components(
                &mut ts,
                summary,
                options,
                &global_components,
                &syntactic_type_only_imported_names,
            );

            // In projects that opt into unused-local diagnostics, this list is
            // narrowed to template-referenced names so user TS6133 can surface.
            profile!(
                "canon.virtual_ts.emit_setup_binding_anchors",
                emit_setup_binding_anchors(
                    &mut ts,
                    summary,
                    script_content,
                    template_referenced_names,
                    reference_setup_bindings_comment,
                )
            );

            ts.push_str("  })();\n");
        });
    }

    if has_template_scope && !check_options.check_template_bindings && preserve_unused_diagnostics {
        profile!(
            "canon.virtual_ts.emit_no_check_template_binding_anchors",
            emit_setup_binding_anchors(
                &mut ts,
                summary,
                script_content,
                template_referenced_names,
                reference_setup_bindings_comment,
            )
        );
    }

    emit_setup_scope_macro_anchors(
        &mut ts,
        summary,
        script_content,
        template_referenced_names,
        preserve_unused_diagnostics,
    );

    let define_emits_runtime_args = summary.macros.define_emits().and_then(|call| {
        if call.type_args.is_none() {
            call.runtime_args.as_ref()
        } else {
            None
        }
    });

    let mut setup_return_fields: Vec<String> = Vec::new();
    self::script_module::push_setup_return_fields(&named_value_exports, &mut setup_return_fields);
    namespace_hoist.push_captured_return_fields(&named_value_exports, &mut setup_return_fields);
    setup_type_exports.emit_setup_artifacts(&mut ts, &mut setup_return_fields);
    let mut setup_artifact_return_fields = Vec::new();
    setup_props_plan.push_return_field(&mut setup_artifact_return_fields);
    setup_return_fields.extend(setup_artifact_return_fields.into_iter().map(String::from));
    let preserve_authored_component =
        generation_options.preserves_authored_component(declared_default_alias, has_script_setup);
    if preserve_authored_component
        && !setup_return_fields
            .iter()
            .any(|field| field == "__default__")
    {
        setup_return_fields.push("__default__".into());
    }
    if let Some(expose) = summary.macros.define_expose()
        && expose.type_args.is_none()
        && let Some(runtime_args) = expose.runtime_args.as_ref()
    {
        append!(ts, "\n  const __vize_exposed = ({runtime_args});\n");
        setup_return_fields.push("__vize_exposed".into());
    }
    if let Some(runtime_args) = define_emits_runtime_args {
        append!(
            ts,
            "\n  const __vize_emit_options = ({runtime_args});\n  const __vize_emits = defineEmits(__vize_emit_options);\n"
        );
        setup_return_fields.push("__vize_emit_options".into());
        setup_return_fields.push("__vize_emits".into());
    }
    setup_props_plan.emit_options_api_artifact(&mut ts, options_api_props.as_ref());
    if !setup_return_fields.is_empty() {
        append!(ts, "\n  return {{ {} }};\n", setup_return_fields.join(", "));
    }

    ts.push_str("}\n\n");

    // Invoke setup to keep diagnostics inside the generated setup body.
    ts.push_str("// Invoke setup to verify types\n");
    script_module::emit_exports(&mut ts, &mut mappings, &named_value_exports, script_offset);
    setup_type_exports.emit_module_exports(&mut ts);
    setup_props_plan.emit_module_export(&mut ts, options_api_props.as_ref());
    emit_authored_component_aliases(&mut ts, preserve_authored_component);
    let emits_info = emit_emits_type(
        &mut ts,
        summary,
        MacroTypeMappings::new(&mut mappings, script_content, &script_source_offset),
        generation_options.preserve_event_navigation,
        generic_param,
        define_emits_runtime_args.is_some(),
    );
    let slots_is_generic = emit_slots_type(&mut ts, summary, generic_injection.as_ref());
    let (has_exposed_type, exposed_is_generic) =
        emit_exposed_type(&mut ts, summary, generic_injection.as_ref());
    ts.push('\n');
    emit_emit_props_helper(&mut ts, &emits_info, hoist_shared_preamble);

    let generic_component_params = setup_props_plan.generic_component_params(generic_param);
    emit_component_constructors(
        &mut ts,
        &setup_props_plan,
        &ComponentInstanceAliases {
            generic_params: generic_component_params.as_ref(),
            slots_is_generic,
            emits_is_generic: emits_info.has_generic_emits(),
            exposed_is_generic,
            has_emits_for_props: emits_info.has_emits_for_props,
            has_exposed_type,
            has_authored_default: preserve_authored_component,
        },
        legacy_vue2,
        dialect,
    );
    let static_raw_props_ref = (!legacy_vue2::needs_legacy_vue2_helpers(legacy_vue2, dialect))
        .then(|| {
            setup_props_plan.component_value_props_type_ref(generic_component_params.as_ref())
        });
    emit_default_export_declaration(
        &mut ts,
        &emits_info,
        generic_component_params
            .as_ref()
            .map(|(decl, names)| (decl.as_str(), names.as_str(), slots_is_generic)),
        preserve_authored_component,
        static_raw_props_ref.as_deref(),
        (summary.macros.define_slots().is_some() && !slots_is_generic).then_some("Slots"),
        self::fallthrough::fallthrough_props_type_ref(summary, template_ast, legacy_vue2)
            .as_deref(),
    );
    component_export::emit_component_default_export(&mut ts, generation_options.component_name);

    VirtualTsOutput {
        code: ts,
        mappings,
        semantic_links,
    }
}
