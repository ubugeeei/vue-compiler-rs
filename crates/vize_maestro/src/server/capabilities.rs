//! LSP server capabilities declaration.
#![allow(clippy::disallowed_methods)]

use tower_lsp::lsp_types::{
    CallHierarchyServerCapability, CodeActionKind, CodeActionOptions, CodeActionProviderCapability,
    CodeLensOptions, ColorProviderCapability, CompletionOptions, DeclarationCapability,
    DocumentLinkOptions, DocumentOnTypeFormattingOptions, FileOperationFilter,
    FileOperationPattern, FileOperationPatternKind, FileOperationRegistrationOptions,
    FoldingRangeProviderCapability, HoverProviderCapability, ImplementationProviderCapability,
    LinkedEditingRangeServerCapabilities, OneOf, RenameOptions, SaveOptions,
    SelectionRangeProviderCapability, SemanticTokenModifier, SemanticTokenType,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TypeDefinitionProviderCapability, WorkDoneProgressOptions,
    WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};

use super::state::LspFeatureConfig;

/// Build the server capabilities to advertise to the client.
pub fn server_capabilities(features: LspFeatureConfig) -> ServerCapabilities {
    ServerCapabilities {
        // Document synchronization
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
            },
        )),

        // Hover support
        hover_provider: features
            .hover
            .then_some(HoverProviderCapability::Simple(true)),

        // Completion support
        completion_provider: features.completion.then_some(CompletionOptions {
            trigger_characters: Some(crate::ide::trigger_characters()),
            resolve_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            all_commit_characters: None,
            completion_item: None,
        }),

        // Go to definition
        definition_provider: features.definition.then_some(OneOf::Left(true)),

        // Find references
        references_provider: features.references.then_some(OneOf::Left(true)),

        // Highlight matching tags and symbol occurrences in the current document.
        document_highlight_provider: features.references.then_some(OneOf::Left(true)),

        // Document symbols (outline)
        document_symbol_provider: features.document_symbols.then_some(OneOf::Left(true)),

        // Workspace symbols
        workspace_symbol_provider: features.workspace_symbols.then_some(OneOf::Left(true)),

        // Code actions (quick fixes, refactoring)
        code_action_provider: (features.lint && features.code_actions).then_some(
            CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: Some(false),
            }),
        ),

        // Rename support
        rename_provider: features.rename.then_some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),

        // Linked editing keeps an open/close tag-name pair in sync as the user
        // types. It is rename-as-you-type, so it rides the `rename` flag rather
        // than introducing a second user-facing setting for the same intent.
        linked_editing_range_provider: features
            .rename
            .then_some(LinkedEditingRangeServerCapabilities::Simple(true)),

        // Document formatting
        document_formatting_provider: features.formatting.then_some(OneOf::Left(true)),

        // Range formatting ("Format Selection"). Rides the `formatting` flag
        // with `document_formatting_provider`: the handler projects the
        // whole-document format back onto the SFC blocks the selection
        // intersects, so the two commands can never disagree.
        document_range_formatting_provider: features.formatting.then_some(OneOf::Left(true)),

        // On-type formatting re-indents the line as a `;`, `}` or newline is
        // typed. Same trigger set as `@vue/language-server` so an editor
        // configured for one server behaves identically under the other. Rides
        // the `formatting` flag with its two siblings above: it is the same
        // formatter again, narrowed to the line under the caret.
        document_on_type_formatting_provider: features.formatting.then(|| {
            DocumentOnTypeFormattingOptions {
                first_trigger_character: ";".to_string(),
                more_trigger_character: Some(vec!["}".to_string(), "\n".to_string()]),
            }
        }),

        // TypeScript/tsgo signature help through the canonical virtual project.
        signature_help_provider: (features.signature_help && cfg!(feature = "native")).then(|| {
            SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_string(), ",".to_string(), "<".to_string()]),
                retrigger_characters: Some(vec![")".to_string()]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }
        }),

        // Code lens
        code_lens_provider: features.code_lens.then_some(CodeLensOptions {
            resolve_provider: Some(false),
        }),

        // Semantic tokens (syntax highlighting)
        semantic_tokens_provider: features.semantic_tokens.then_some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: vec![
                        SemanticTokenType::NAMESPACE,
                        SemanticTokenType::TYPE,
                        SemanticTokenType::CLASS,
                        SemanticTokenType::ENUM,
                        SemanticTokenType::INTERFACE,
                        SemanticTokenType::STRUCT,
                        SemanticTokenType::TYPE_PARAMETER,
                        SemanticTokenType::PARAMETER,
                        SemanticTokenType::VARIABLE,
                        SemanticTokenType::PROPERTY,
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenType::EVENT,
                        SemanticTokenType::FUNCTION,
                        SemanticTokenType::METHOD,
                        SemanticTokenType::MACRO,
                        SemanticTokenType::KEYWORD,
                        SemanticTokenType::MODIFIER,
                        SemanticTokenType::COMMENT,
                        SemanticTokenType::STRING,
                        SemanticTokenType::NUMBER,
                        SemanticTokenType::REGEXP,
                        SemanticTokenType::OPERATOR,
                        SemanticTokenType::DECORATOR,
                    ],
                    token_modifiers: vec![
                        SemanticTokenModifier::DECLARATION,
                        SemanticTokenModifier::DEFINITION,
                        SemanticTokenModifier::READONLY,
                        SemanticTokenModifier::STATIC,
                        SemanticTokenModifier::DEPRECATED,
                        SemanticTokenModifier::ABSTRACT,
                        SemanticTokenModifier::ASYNC,
                        SemanticTokenModifier::MODIFICATION,
                        SemanticTokenModifier::DOCUMENTATION,
                        SemanticTokenModifier::DEFAULT_LIBRARY,
                    ],
                },
                range: Some(true),
                full: Some(SemanticTokensFullOptions::Bool(true)),
            }),
        ),

        // Document links
        document_link_provider: features.document_links.then_some(DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),

        // Folding ranges
        folding_range_provider: features
            .folding_ranges
            .then_some(FoldingRangeProviderCapability::Simple(true)),

        // Selection ranges (expand/shrink selection). Gated on the same flag as
        // folding ranges: both are authored-document *structure* features built
        // from the SFC block layout and the template markup, so
        // `vize.foldingRanges.enable` is the single user-facing control for the
        // structure group — mirroring how `references` gates both
        // `referencesProvider` and `documentHighlightProvider` above.
        selection_range_provider: features
            .folding_ranges
            .then_some(SelectionRangeProviderCapability::Simple(true)),

        // Inlay hints
        inlay_hint_provider: features.inlay_hints.then_some(OneOf::Left(true)),

        // Workspace capabilities
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: workspace_file_operations(features),
        }),

        // Colour swatches and the picker they open, for the CSS a `.vue` file
        // authors. Gated on the same flag as document links: both decorate a
        // literal in the authored text and make it interactive — a path you can
        // follow, a colour you can pick — and neither needs the type checker.
        color_provider: features
            .document_links
            .then_some(ColorProviderCapability::Simple(true)),

        // Checker-backed type-definition, implementation, and declaration use
        // Corsa directly: the providers stay hidden when typecheck is disabled
        // because no lexical fallback can answer their semantics.
        type_definition_provider: (features.definition
            && features.typecheck
            && cfg!(feature = "native"))
        .then_some(TypeDefinitionProviderCapability::Simple(true)),
        implementation_provider: (features.definition
            && features.typecheck
            && cfg!(feature = "native"))
        .then_some(ImplementationProviderCapability::Simple(true)),
        declaration_provider: (features.definition
            && features.typecheck
            && cfg!(feature = "native"))
        .then_some(DeclarationCapability::Simple(true)),
        call_hierarchy_provider: (features.definition
            && features.typecheck
            && cfg!(feature = "native"))
        .then_some(CallHierarchyServerCapability::Simple(true)),
        // Features not yet implemented
        execute_command_provider: None,
        moniker_provider: None,
        experimental: features.auto_insert.then(|| {
            serde_json::json!({
                "autoInsertionProvider": {
                    "triggerCharacters": ["}", "=", ">", "/", "\\w"],
                    "configurationSections": [
                        ["vize.autoInsert.bracketSpacing"],
                        ["vize.autoInsert.autoCreateQuotes"],
                        ["vize.autoInsert.autoClosingTags"],
                        ["vize.autoInsert.dotValue"]
                    ]
                }
            })
        }),

        // Default for other fields
        ..Default::default()
    }
}

fn workspace_file_operations(
    features: LspFeatureConfig,
) -> Option<WorkspaceFileOperationsServerCapabilities> {
    let track_declarations = cfg!(feature = "native") && features.typecheck;
    let track_vue_symbols = cfg!(feature = "native") && features.workspace_symbols;
    let track_file_events = track_declarations || track_vue_symbols;
    if !track_file_events && !features.file_rename {
        return None;
    }
    Some(WorkspaceFileOperationsServerCapabilities {
        did_create: track_file_events
            .then(|| file_event_registration_options(track_declarations, track_vue_symbols)),
        will_create: None,
        did_rename: Some(if features.file_rename {
            file_rename_registration_options()
        } else {
            file_event_registration_options(track_declarations, track_vue_symbols)
        }),
        will_rename: features.file_rename.then(file_rename_registration_options),
        did_delete: track_file_events
            .then(|| file_event_registration_options(track_declarations, track_vue_symbols)),
        will_delete: None,
    })
}

fn file_event_registration_options(
    track_declarations: bool,
    track_vue_symbols: bool,
) -> FileOperationRegistrationOptions {
    let mut filters = Vec::with_capacity(2);
    if track_declarations {
        filters.push(FileOperationFilter {
            scheme: Some("file".to_string()),
            pattern: FileOperationPattern {
                glob: "**/*.d.{ts,mts,cts}".to_string(),
                matches: Some(FileOperationPatternKind::File),
                options: None,
            },
        });
    }
    if track_vue_symbols {
        filters.push(FileOperationFilter {
            scheme: Some("file".to_string()),
            pattern: FileOperationPattern {
                glob: "**/*.vue".to_string(),
                matches: Some(FileOperationPatternKind::File),
                options: None,
            },
        });
    }
    filters.push(FileOperationFilter {
        scheme: Some("file".to_string()),
        pattern: FileOperationPattern {
            glob: "**/*".to_string(),
            matches: Some(FileOperationPatternKind::Folder),
            options: None,
        },
    });
    FileOperationRegistrationOptions { filters }
}

fn file_rename_registration_options() -> FileOperationRegistrationOptions {
    FileOperationRegistrationOptions {
        filters: vec![
            FileOperationFilter {
                scheme: Some("file".to_string()),
                pattern: FileOperationPattern {
                    glob: "**/*.{vue,ts,tsx,d.ts,d.mts,d.cts,js,jsx,mts,cts,mjs,cjs}".to_string(),
                    matches: Some(FileOperationPatternKind::File),
                    options: None,
                },
            },
            FileOperationFilter {
                scheme: Some("file".to_string()),
                pattern: FileOperationPattern {
                    glob: "**/*".to_string(),
                    matches: Some(FileOperationPatternKind::Folder),
                    options: None,
                },
            },
        ],
    }
}

#[cfg(test)]
mod code_action_tests;
#[cfg(test)]
mod file_operation_tests;
#[cfg(test)]
mod tests;
