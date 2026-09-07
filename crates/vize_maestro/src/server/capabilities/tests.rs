use super::*;
use crate::ide::trigger_characters;

fn all_features() -> LspFeatureConfig {
    LspFeatureConfig {
        lint: true,
        typecheck: true,
        ecosystem: true,
        options_api: true,
        legacy_vue2: true,
        completion: true,
        signature_help: true,
        hover: true,
        definition: true,
        references: true,
        document_symbols: true,
        workspace_symbols: true,
        code_actions: true,
        rename: true,
        formatting: true,
        code_lens: true,
        semantic_tokens: true,
        document_links: true,
        folding_ranges: true,
        inlay_hints: true,
        file_rename: true,
        auto_insert: true,
        cross_file: true,
    }
}

#[test]
fn text_document_sync_uses_incremental_open_close_and_save_without_text() {
    let capabilities = server_capabilities(LspFeatureConfig::default());
    let sync = capabilities
        .text_document_sync
        .expect("text document sync should be advertised");

    let TextDocumentSyncCapability::Options(options) = sync else {
        panic!("text document sync must use explicit options");
    };

    assert_eq!(options.open_close, Some(true));
    assert_eq!(options.change, Some(TextDocumentSyncKind::INCREMENTAL));
    assert_eq!(options.will_save, Some(false));
    assert_eq!(options.will_save_wait_until, Some(false));

    let Some(TextDocumentSyncSaveOptions::SaveOptions(save)) = options.save else {
        panic!("save options should be advertised");
    };
    assert_eq!(save.include_text, Some(false));
}

#[test]
fn completion_triggers_match_completion_service_contract() {
    let capabilities = server_capabilities(LspFeatureConfig::default());
    let advertised = capabilities
        .completion_provider
        .expect("completion should be advertised")
        .trigger_characters
        .expect("completion triggers should be advertised");

    assert_eq!(advertised, trigger_characters());
}

mod feature_gating;

#[test]
fn default_features_advertise_non_opinionated_providers() {
    let capabilities = server_capabilities(LspFeatureConfig::default());

    assert!(capabilities.signature_help_provider.is_some());
    assert_eq!(
        capabilities.type_definition_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.implementation_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.declaration_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.call_hierarchy_provider.is_some(),
        cfg!(feature = "native")
    );
    assert!(matches!(
        capabilities.selection_range_provider,
        Some(SelectionRangeProviderCapability::Simple(true))
    ));
    assert!(capabilities.completion_provider.is_some());
    assert!(capabilities.hover_provider.is_some());
    assert!(capabilities.definition_provider.is_some());
    assert!(capabilities.document_link_provider.is_some());
    assert!(capabilities.document_formatting_provider.is_none());
    assert!(capabilities.document_range_formatting_provider.is_none());
    assert!(capabilities.document_on_type_formatting_provider.is_none());
}

/// The trigger set is `@vue/language-server`'s, character for character, so an
/// editor configured for one server behaves the same under the other (#3456).
#[test]
fn on_type_formatting_advertises_the_vue_language_server_trigger_set() {
    let options = server_capabilities(all_features())
        .document_on_type_formatting_provider
        .expect("on-type formatting rides the formatting flag");

    assert_eq!(options.first_trigger_character, ";");
    assert_eq!(
        options.more_trigger_character,
        Some(vec!["}".to_string(), "\n".to_string()])
    );
}

#[test]
fn auto_insertion_advertises_positional_vscode_configuration_sections() {
    let experimental = server_capabilities(all_features())
        .experimental
        .expect("auto insertion should advertise a private client capability");

    assert_eq!(
        experimental,
        serde_json::json!({
            "autoInsertionProvider": {
                "triggerCharacters": ["}", "=", ">", "/", "\\w"],
                "configurationSections": [
                    ["vize.autoInsert.bracketSpacing"],
                    ["vize.autoInsert.autoCreateQuotes"],
                    ["vize.autoInsert.autoClosingTags"],
                    ["vize.autoInsert.autoClosingTags"],
                    ["vize.autoInsert.dotValue"]
                ]
            }
        })
    );
}

#[test]
fn all_features_skip_unimplemented_providers_and_keep_implemented_ones() {
    let capabilities = server_capabilities(all_features());

    let signature_help = capabilities
        .signature_help_provider
        .expect("signature help should be advertised");
    assert_eq!(
        signature_help.trigger_characters,
        Some(vec!["(".to_string(), ",".to_string(), "<".to_string()])
    );
    assert_eq!(
        signature_help.retrigger_characters,
        Some(vec![")".to_string()])
    );
    assert!(matches!(
        capabilities.selection_range_provider,
        Some(SelectionRangeProviderCapability::Simple(true))
    ));
    assert_eq!(
        capabilities
            .document_link_provider
            .as_ref()
            .and_then(|provider| provider.resolve_provider),
        Some(false)
    );

    assert!(capabilities.completion_provider.is_some());
    assert!(capabilities.hover_provider.is_some());
    assert!(capabilities.definition_provider.is_some());
    assert_eq!(
        capabilities.type_definition_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.implementation_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.declaration_provider.is_some(),
        cfg!(feature = "native")
    );
    assert_eq!(
        capabilities.call_hierarchy_provider.is_some(),
        cfg!(feature = "native")
    );
    assert!(capabilities.references_provider.is_some());
    assert!(capabilities.document_symbol_provider.is_some());
    assert!(capabilities.workspace_symbol_provider.is_some());
    assert!(capabilities.code_action_provider.is_some());
    assert!(capabilities.rename_provider.is_some());
    assert!(capabilities.document_formatting_provider.is_some());
    assert!(capabilities.document_range_formatting_provider.is_some());
    assert!(capabilities.code_lens_provider.is_some());
    assert!(capabilities.semantic_tokens_provider.is_some());
    assert!(capabilities.document_link_provider.is_some());
    assert!(capabilities.folding_range_provider.is_some());
    assert!(capabilities.inlay_hint_provider.is_some());
    assert!(
        capabilities
            .workspace
            .and_then(|workspace| workspace.file_operations)
            .is_some()
    );
}

#[test]
fn auto_insert_experimental_sections_are_unique_and_complete() {
    let experimental = server_capabilities(all_features())
        .experimental
        .expect("auto insert should advertise experimental capabilities");
    let sections = experimental
        .pointer("/autoInsertionProvider/configurationSections")
        .and_then(serde_json::Value::as_array)
        .expect("auto insert should advertise configuration sections");

    let section_names: Vec<&str> = sections
        .iter()
        .map(|section| {
            section
                .as_array()
                .and_then(|section| section.first())
                .and_then(serde_json::Value::as_str)
                .expect("configuration section should contain one setting key")
        })
        .collect();

    assert_eq!(
        section_names,
        [
            "vize.autoInsert.bracketSpacing",
            "vize.autoInsert.autoCreateQuotes",
            "vize.autoInsert.autoClosingTags",
            "vize.autoInsert.dotValue",
        ]
    );

    let mut unique_names = section_names.clone();
    unique_names.sort_unstable();
    unique_names.dedup();
    assert_eq!(unique_names.len(), section_names.len());
}

#[test]
fn file_rename_registration_mentions_declaration_files() {
    let options = file_rename_registration_options();
    let file_glob = &options.filters[0].pattern.glob;

    for extension in ["d.ts", "d.mts", "d.cts"] {
        assert!(
            file_glob.contains(extension),
            "rename operations should include declaration shims: {file_glob}"
        );
    }
}

#[test]
fn declaration_file_operations_cover_create_delete_and_rename() {
    let mut features = all_features();
    features.file_rename = false;
    let operations = server_capabilities(features)
        .workspace
        .and_then(|workspace| workspace.file_operations)
        .expect("declaration tracking should be advertised");

    for options in [
        operations.did_create,
        operations.did_delete,
        operations.did_rename,
    ] {
        let options = options.expect("every declaration event must be registered");
        assert_eq!(options.filters.len(), 3);
        assert_eq!(options.filters[0].pattern.glob, "**/*.d.{ts,mts,cts}");
        assert_eq!(options.filters[1].pattern.glob, "**/*.vue");
        assert_eq!(options.filters[2].pattern.glob, "**/*");
        assert!(options.filters[2].pattern.matches == Some(FileOperationPatternKind::Folder));
    }
}
