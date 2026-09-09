use super::parse_script_setup;

#[test]
fn static_emits_retain_their_written_name_ranges() {
    for (source, name, declaration) in [
        (
            "defineEmits<{ (event: \"submit\", accepted: boolean): void }>()",
            "submit",
            "\"submit\"",
        ),
        ("defineEmits<{ save: [value: number] }>()", "save", "save"),
        (
            "defineEmits({ cancel: (reason: string) => true })",
            "cancel",
            "cancel",
        ),
    ] {
        let result = parse_script_setup(source);
        let (start, end) = result
            .macros
            .emit_declaration(name)
            .expect("static event declaration range");
        assert_eq!(&source[start as usize..end as usize], declaration);
    }
}

#[test]
fn shifting_macros_keeps_event_declarations_in_the_script_coordinate_space() {
    let source = "defineEmits(['save'])";
    let mut result = parse_script_setup(source);
    let declaration = result
        .macros
        .emit_declaration("save")
        .expect("event declaration");

    result.macros.shift_offsets(17);

    assert_eq!(
        result.macros.emit_declaration("save"),
        Some((declaration.0 + 17, declaration.1 + 17))
    );
}

#[test]
fn models_retain_explicit_names_and_default_macro_ranges() {
    for (source, name, local_name, declaration) in [
        (
            "const titleRef = defineModel<string>(\"title\")",
            "title",
            "titleRef",
            "\"title\"",
        ),
        (
            "const model = defineModel<number>()",
            "modelValue",
            "model",
            "defineModel",
        ),
    ] {
        let result = parse_script_setup(source);
        let model = result
            .macros
            .models()
            .iter()
            .find(|model| model.name == name)
            .expect("model definition");
        assert_eq!(model.local_name, local_name);
        let (start, end) = result
            .macros
            .model_declaration(name)
            .expect("model declaration range");
        assert_eq!(&source[start as usize..end as usize], declaration);
    }
}

#[test]
fn array_destructured_define_model_keeps_public_model_contract() {
    let source = "const [model, modifiers] = defineModel<string, \"trim\" | \"capitalize\">({ required: true })";
    let result = parse_script_setup(source);
    let models = result.macros.models();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "modelValue");
    assert_eq!(models[0].local_name, "model");
    assert_eq!(models[0].model_type.as_deref(), Some("string"));
    assert_eq!(
        result.macros.model_modifier_type(models[0].name.as_str()),
        Some("\"trim\" | \"capitalize\"")
    );
    assert!(models[0].required);
}

#[test]
fn define_model_runtime_constructor_type_is_recorded() {
    let result = parse_script_setup("const model = defineModel({ type: String, default: '' })");
    let models = result.macros.models();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_type.as_deref(), Some("string"));
    assert!(models[0].default_value.is_some());
}

#[test]
fn define_model_runtime_constructor_array_type_is_recorded() {
    let result = parse_script_setup("const model = defineModel({ type: [String, Number] })");
    let models = result.macros.models();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_type.as_deref(), Some("string | number"));
}

#[test]
fn shifting_macros_keeps_model_declarations_in_script_coordinates() {
    let mut result = parse_script_setup("defineModel<string>(\"title\")");
    let declaration = result
        .macros
        .model_declaration("title")
        .expect("model declaration");

    result.macros.shift_offsets(23);

    assert_eq!(
        result.macros.model_declaration("title"),
        Some((declaration.0 + 23, declaration.1 + 23))
    );
}
