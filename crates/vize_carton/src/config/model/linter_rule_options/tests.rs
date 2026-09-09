use super::{
    ComponentNameInTemplateCasingOptions, ConfigLintRuleOptions, CustomEventNameCasing,
    CustomEventNameCasingOptions, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions,
    HtmlSelfClosingStyle, LintRuleOptions, MuseaDesignToken, RestrictedGlobal, RestrictedMember,
    TemplateComponentNameCasing,
};

#[test]
fn empty_options_deserialize_to_default() {
    let options = serde_json::from_str::<LintRuleOptions>("{}").unwrap();
    assert_eq!(options, LintRuleOptions::default());
    assert!(options.is_empty());
}

#[test]
fn deserializes_restricted_globals_with_and_without_message() {
    let json = r#"{
        "script/no-restricted-globals": {
            "globals": [
                { "name": "process", "message": "Use a typed config helper." },
                { "name": "localStorage" }
            ]
        }
    }"#;
    let options = serde_json::from_str::<LintRuleOptions>(json).unwrap();
    let globals = options.no_restricted_globals.unwrap().globals;
    assert_eq!(
        globals,
        vec![
            RestrictedGlobal {
                name: "process".into(),
                message: Some("Use a typed config helper.".into()),
            },
            RestrictedGlobal {
                name: "localStorage".into(),
                message: None,
            },
        ]
    );
    assert!(options.no_restricted_members.is_none());
}

#[test]
fn deserializes_restricted_members() {
    let json = r#"{
        "script/no-restricted-members": {
            "members": [
                { "object": "window", "property": "localStorage", "message": "Use authStorage." },
                { "object": "globalThis", "property": "process" }
            ]
        }
    }"#;
    let options = serde_json::from_str::<LintRuleOptions>(json).unwrap();
    let members = options.no_restricted_members.unwrap().members;
    assert_eq!(
        members,
        vec![
            RestrictedMember {
                object: "window".into(),
                property: "localStorage".into(),
                message: Some("Use authStorage.".into()),
            },
            RestrictedMember {
                object: "globalThis".into(),
                property: "process".into(),
                message: None,
            },
        ]
    );
    assert!(options.no_restricted_globals.is_none());
}

#[test]
fn deserializes_casing_options() {
    let json = r#"{
        "vue/component-name-in-template-casing": { "casing": "kebab-case" },
        "script/custom-event-name-casing": { "casing": "camelCase" }
    }"#;
    let options = serde_json::from_str::<ConfigLintRuleOptions>(json).unwrap();
    assert_eq!(
        options.component_name_in_template_casing,
        Some(ComponentNameInTemplateCasingOptions {
            casing: TemplateComponentNameCasing::KebabCase
        })
    );
    assert_eq!(
        options.custom_event_name_casing,
        Some(CustomEventNameCasingOptions {
            casing: CustomEventNameCasing::CamelCase
        })
    );
    assert_eq!(
        options.component_name_in_template_casing(),
        Some(TemplateComponentNameCasing::KebabCase)
    );
    assert_eq!(
        options.custom_event_name_casing(),
        Some(CustomEventNameCasing::CamelCase)
    );
}

#[test]
fn deserializes_html_self_closing_options() {
    let json = r#"{
        "vue/html-self-closing": {
            "html": {
                "void": "any",
                "normal": "never",
                "component": "any"
            },
            "svg": "any",
            "math": "any"
        }
    }"#;
    let options = serde_json::from_str::<ConfigLintRuleOptions>(json).unwrap();
    assert_eq!(
        options.html_self_closing(),
        Some(HtmlSelfClosingOptions {
            html: HtmlSelfClosingHtmlOptions {
                void_elements: HtmlSelfClosingStyle::Any,
                normal: HtmlSelfClosingStyle::Never,
                component: HtmlSelfClosingStyle::Any,
            },
            svg: HtmlSelfClosingStyle::Any,
            math: HtmlSelfClosingStyle::Any,
        })
    );
}

#[test]
fn partial_html_self_closing_options_keep_vize_defaults() {
    let json = r#"{
        "vue/html-self-closing": {
            "html": { "normal": "never" }
        }
    }"#;
    let options = serde_json::from_str::<ConfigLintRuleOptions>(json).unwrap();
    assert_eq!(
        options.html_self_closing(),
        Some(HtmlSelfClosingOptions {
            html: HtmlSelfClosingHtmlOptions {
                void_elements: HtmlSelfClosingStyle::Always,
                normal: HtmlSelfClosingStyle::Never,
                component: HtmlSelfClosingStyle::Always,
            },
            svg: HtmlSelfClosingStyle::Always,
            math: HtmlSelfClosingStyle::Always,
        })
    );
}

#[test]
fn deserializes_musea_design_tokens_with_default_tier() {
    let json = r##"{
        "musea/prefer-design-tokens": {
            "tokens": [
                { "path": "color.primary", "value": "#3b82f6" },
                { "path": "color.danger", "value": "#ef4444", "tier": "semantic" }
            ]
        }
    }"##;
    let options = serde_json::from_str::<ConfigLintRuleOptions>(json).unwrap();
    let tokens = options.musea_prefer_design_tokens.unwrap().tokens;
    assert_eq!(
        tokens,
        vec![
            MuseaDesignToken {
                path: "color.primary".into(),
                value: "#3b82f6".into(),
                tier: "primitive".into(),
            },
            MuseaDesignToken {
                path: "color.danger".into(),
                value: "#ef4444".into(),
                tier: "semantic".into(),
            },
        ]
    );
}

#[test]
fn stable_lint_rule_options_keep_legacy_shape() {
    let json = r#"{
        "script/no-restricted-globals": {
            "globals": [{ "name": "process" }]
        },
        "vue/component-name-in-template-casing": { "casing": "kebab-case" }
    }"#;
    let options = serde_json::from_str::<ConfigLintRuleOptions>(json).unwrap();
    assert_eq!(
        options.stable_options().restricted_globals(),
        [("process".into(), None)]
    );
    assert!(options.component_name_in_template_casing().is_some());
}

#[test]
fn unknown_fields_are_rejected() {
    let json = r#"{
        "script/no-restricted-globals": {
            "globals": [{ "name": "process", "bogus": true }]
        }
    }"#;
    assert!(serde_json::from_str::<LintRuleOptions>(json).is_err());
}

#[test]
fn unknown_musea_option_fields_are_rejected() {
    let json = r##"{
        "musea/prefer-design-tokens": {
            "toknes": [
                { "path": "color.primary", "value": "#3b82f6" }
            ]
        }
    }"##;
    assert!(serde_json::from_str::<ConfigLintRuleOptions>(json).is_err());
}

#[test]
fn invalid_html_self_closing_options_are_rejected() {
    let invalid_enum = r#"{
        "vue/html-self-closing": {
            "html": { "normal": "sometimes" }
        }
    }"#;
    assert!(serde_json::from_str::<ConfigLintRuleOptions>(invalid_enum).is_err());

    let unknown_field = r#"{
        "vue/html-self-closing": {
            "html": { "normalHtml": "never" }
        }
    }"#;
    assert!(serde_json::from_str::<ConfigLintRuleOptions>(unknown_field).is_err());
}
