//! Tests for script compilation.

#[cfg(test)]
mod tests {
    use crate::compile_script::compile_script;
    use crate::compile_script::function_mode::compile_script_setup;
    use crate::compile_script::props::{
        extract_prop_types_from_type, extract_with_defaults_defaults, is_valid_identifier,
    };
    use crate::compile_script::typescript::transform_typescript_to_js;
    use crate::types::SfcDescriptor;

    #[test]
    fn test_compile_empty_script() {
        let descriptor = SfcDescriptor::default();
        let result =
            compile_script(&descriptor, &Default::default(), "Test", false, false).unwrap();
        assert!(result.code.contains("__sfc__"));
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("$baz"));
        assert!(is_valid_identifier("foo123"));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("foo-bar"));
    }

    #[test]
    fn test_extract_with_defaults_defaults() {
        // Test simple case
        let input = r#"withDefaults(defineProps<{ msg?: string }>(), { msg: "hello" })"#;
        let defaults = extract_with_defaults_defaults(input);
        eprintln!("Defaults: {:?}", defaults);
        assert_eq!(defaults.get("msg"), Some(&r#""hello""#.to_string()));

        // Test multiple defaults
        let input2 = r#"withDefaults(defineProps<{ msg?: string, count?: number }>(), { msg: "hello", count: 42 })"#;
        let defaults2 = extract_with_defaults_defaults(input2);
        assert_eq!(defaults2.get("msg"), Some(&r#""hello""#.to_string()));
        assert_eq!(defaults2.get("count"), Some(&"42".to_string()));

        // Test multiline input like AfCheckbox
        let input3 = r#"withDefaults(
  defineProps<{
    checked: boolean;
    label?: string;
    color?: "primary" | "secondary";
  }>(),
  {
    label: undefined,
    color: "primary",
  },
)"#;
        let defaults3 = extract_with_defaults_defaults(input3);
        eprintln!("Defaults3: {:?}", defaults3);
        assert_eq!(defaults3.get("label"), Some(&"undefined".to_string()));
        assert_eq!(defaults3.get("color"), Some(&r#""primary""#.to_string()));
    }

    #[test]
    fn test_compile_script_setup_with_define_props() {
        let content = r#"
import { ref } from 'vue'
const props = defineProps(['msg'])
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // Should have __sfc__
        assert!(
            result.code.contains("const __sfc__ ="),
            "Should have __sfc__"
        );
        // Should have __name
        assert!(result.code.contains("__name: 'Test'"), "Should have __name");
        // Should have props definition
        assert!(
            result.code.contains("props: ['msg']"),
            "Should have props definition"
        );
        // Should have setup function with proper signature
        assert!(
            result
                .code
                .contains("setup(__props, { expose: __expose, emit: __emit })"),
            "Should have proper setup signature"
        );
        // __expose is only called if defineExpose is used (not in this test)
        // Should have __returned__
        assert!(
            result.code.contains("const __returned__ ="),
            "Should have __returned__"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_emits() {
        let content = r#"
const emit = defineEmits(['click', 'update'])
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Full output:\n{}", result.code);

        assert!(
            result.code.contains("emits:"),
            "Should contain emits definition"
        );
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // emit should be in __returned__ as it's a runtime value used in templates
        assert!(
            result.code.contains("emit"),
            "emit should be accessible in template"
        );
        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_emits_usage() {
        let content = r#"
import { ref } from 'vue'
const emit = defineEmits(['click', 'update'])
const count = ref(0)
function onClick() {
    emit('click', count.value)
}
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // defineEmits call should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits call should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // onClick function should be in setup
        assert!(
            result.code.contains("function onClick()"),
            "onClick should be in setup"
        );
        // emits definition should be present
        assert!(
            result.code.contains("emits: ['click', 'update']"),
            "Should have emits definition"
        );
    }

    #[test]
    fn test_compile_script_setup_without_macros() {
        let content = r#"
import { ref } from 'vue'
const msg = ref('hello')
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // Should have setup
        assert!(result.code.contains("setup(__props"), "Should have setup");
        // Should NOT have props or emits definitions
        assert!(
            !result.code.contains("  props:"),
            "Should not contain props"
        );
        assert!(!result.code.contains("emits:"), "Should not contain emits");
    }

    #[test]
    fn test_compile_script_setup_with_props_destructure() {
        let content = r#"
import { computed } from 'vue'
const { count } = defineProps({ count: Number })
const double = computed(() => count * 2)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Compiled output:\n{}", result.code);

        // Should transform count to __props.count inside computed
        assert!(
            result.code.contains("__props.count"),
            "Should transform destructured prop to __props.count"
        );
        // The original `count` reference should be replaced
        assert!(
            result.code.contains("computed(() => __props.count * 2)"),
            "Should have transformed computed expression"
        );
        // Destructured props should NOT be in __returned__
        assert!(
            !result.code.contains("__returned__ = { computed, count,"),
            "Destructured props should not be in __returned__"
        );
        // Should have double and computed in __returned__
        assert!(
            result.code.contains("computed") && result.code.contains("double"),
            "Should have computed and double in __returned__"
        );
    }

    #[test]
    fn test_compiler_macros_not_in_returned() {
        let content = r#"
import { defineProps, ref } from 'vue'
const props = defineProps(['msg'])
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Compiled output:\n{}", result.code);

        // Find the __returned__ line and check its contents
        let returned_line = result
            .code
            .lines()
            .find(|line| line.contains("__returned__"))
            .expect("Should have __returned__ line");

        println!("__returned__ line: {}", returned_line);

        // Compiler macros should NOT be in __returned__
        assert!(
            !returned_line.contains("defineProps"),
            "Compiler macros should not be in __returned__"
        );
        // But regular imports should be
        assert!(
            returned_line.contains("ref"),
            "Regular imports should be in __returned__"
        );
    }

    #[test]
    fn test_props_destructure_with_defaults() {
        let content = r#"
import { computed, watch } from 'vue'

const {
  name,
  count = 0,
  disabled = false,
  items = () => []
} = defineProps<{
  name: string
  count?: number
  disabled?: boolean
  items?: string[]
}>()

const doubled = computed(() => count * 2)
const itemCount = computed(() => items.length)
"#;

        // First check context analysis
        let mut ctx = crate::script::ScriptCompileContext::new(content);
        ctx.analyze();

        println!("=== Context Analysis ===");
        println!("props_destructure: {:?}", ctx.macros.props_destructure);
        println!("bindings: {:?}", ctx.bindings.bindings);

        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("\n=== Compiled output ===\n{}", result.code);

        // Should NOT contain the destructure statement
        assert!(
            !result.code.contains("const {"),
            "Should not contain destructure statement"
        );
        assert!(
            !result.code.contains("} = defineProps"),
            "Should not contain defineProps assignment"
        );

        // Should have props definition with defaults
        assert!(
            result.code.contains("props:"),
            "Should have props definition"
        );

        // Should transform props to __props
        assert!(
            result.code.contains("__props.count"),
            "Should transform count to __props.count"
        );
        assert!(
            result.code.contains("__props.items"),
            "Should transform items to __props.items"
        );

        // Should have the computed expressions transformed
        assert!(
            result.code.contains("computed(() => __props.count * 2)"),
            "Should transform count in computed. Got:\n{}",
            result.code
        );
    }

    #[test]
    fn test_extract_prop_types() {
        let type_args = r#"{
  name: string
  count?: number
  disabled?: boolean
  items?: string[]
}"#;
        let props = extract_prop_types_from_type(type_args);
        assert!(props.contains_key("name"), "Should extract name");
        assert!(props.contains_key("count"), "Should extract count");
        assert!(props.contains_key("disabled"), "Should extract disabled");
        assert!(props.contains_key("items"), "Should extract items");

        // Check types
        assert_eq!(props.get("name").unwrap().js_type, "String");
        assert_eq!(props.get("count").unwrap().js_type, "Number");
        assert_eq!(props.get("disabled").unwrap().js_type, "Boolean");
        assert_eq!(props.get("items").unwrap().js_type, "Array");

        // Check optionality
        assert!(!props.get("name").unwrap().optional);
        assert!(props.get("count").unwrap().optional);
        assert!(props.get("disabled").unwrap().optional);
        assert!(props.get("items").unwrap().optional);
    }

    #[test]
    fn test_compile_script_setup_with_multiline_define_emits() {
        let content = r#"
const emit = defineEmits<{
  (e: 'click', payload: MouseEvent): void
  (e: 'update', value: string): void
}>()

function handleClick(e: MouseEvent) {
    emit('click', e)
}
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Multi-line defineEmits output:\n{}", result.code);

        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
        // handleClick function should be in setup
        assert!(
            result.code.contains("function handleClick"),
            "handleClick should be in setup"
        );
        // emits definition should be present
        assert!(
            result.code.contains("emits:"),
            "Should have emits definition"
        );
    }

    #[test]
    fn test_compile_script_setup_with_typed_define_emits_single_line() {
        let content = r#"
const emit = defineEmits<(e: 'click') => void>()
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("Typed defineEmits output:\n{}", result.code);

        // defineEmits should NOT be in the setup function
        assert!(
            !result.code.contains("defineEmits"),
            "defineEmits should be removed from setup"
        );
        // emit binding should be present
        assert!(
            result.code.contains("const emit = __emit"),
            "Should bind emit to __emit"
        );
    }

    #[test]
    fn test_compile_script_setup_with_define_expose() {
        let content = r#"
import { ref } from 'vue'
const count = ref(0)
const reset = () => count.value = 0
defineExpose({ count, reset })
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        println!("defineExpose output:\n{}", result.code);

        // defineExpose should be transformed to __expose(...)
        assert!(
            result.code.contains("__expose({"),
            "Should have __expose call with arguments"
        );
        assert!(
            result.code.contains("count"),
            "__expose should include count"
        );
        assert!(
            result.code.contains("reset"),
            "__expose should include reset"
        );
        // defineExpose should NOT be in the setup function
        assert!(
            !result.code.contains("defineExpose"),
            "defineExpose should be removed from setup"
        );
    }

    #[test]
    fn test_compile_script_setup_without_define_expose() {
        let content = r#"
import { ref } from 'vue'
const count = ref(0)
"#;
        let result = compile_script_setup(content, "Test", false, false, None).unwrap();

        // __expose should NOT be called if defineExpose is not used
        assert!(
            !result.code.contains("__expose("),
            "Should not have __expose call without defineExpose"
        );
    }

    #[test]
    fn test_transform_typescript_to_js_strips_types() {
        let ts_code = r#"const getNumber = (x: number): string => {
    return x.toString();
}
const foo: string = "bar";"#;
        let result = transform_typescript_to_js(ts_code);
        eprintln!("TypeScript transform result:\n{}", result);

        // Should NOT contain type annotations
        assert!(
            !result.contains(": number"),
            "Should strip parameter type annotation. Got:\n{}",
            result
        );
        assert!(
            !result.contains(": string"),
            "Should strip return type and variable type annotations. Got:\n{}",
            result
        );
    }

    #[test]
    fn test_compile_script_setup_strips_typescript() {
        let content = r#"
const getNumberOfTeachers = (
  items: Item[]
): string => {
  return items.length.toString();
};
"#;
        let result = compile_script_setup(content, "Test", false, true, None).unwrap();
        eprintln!("Compiled TypeScript output:\n{}", result.code);

        // Should NOT contain type annotations
        assert!(
            !result.code.contains(": Item[]"),
            "Should strip parameter type annotation. Got:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("): string"),
            "Should strip return type annotation. Got:\n{}",
            result.code
        );
    }
}
