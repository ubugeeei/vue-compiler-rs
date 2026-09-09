use vize_carton::{String, append};

/// Runtime prop source used to publish an Options API component's `Props`.
pub(crate) struct OptionsApiPropsSource {
    direct: Option<DirectOptionsApiPropsSource>,
    captures_default: bool,
}

enum DirectOptionsApiPropsSource {
    /// Runtime values that must remain in setup scope.
    DeferredObject(String),
    /// Array-form props, which carry names but no runtime types.
    Names(Vec<String>),
}

impl OptionsApiPropsSource {
    pub(crate) fn deferred_object(source: String) -> Self {
        Self {
            direct: Some(DirectOptionsApiPropsSource::DeferredObject(source)),
            captures_default: false,
        }
    }

    pub(crate) fn names(names: Vec<String>) -> Self {
        Self {
            direct: Some(DirectOptionsApiPropsSource::Names(names)),
            captures_default: false,
        }
    }

    pub(crate) fn default_export() -> Self {
        Self {
            direct: None,
            captures_default: true,
        }
    }

    pub(crate) fn with_default_export(mut self) -> Self {
        self.captures_default = true;
        self
    }

    pub(crate) fn deferred_object_source(&self) -> Option<&str> {
        match self.direct.as_ref() {
            Some(DirectOptionsApiPropsSource::DeferredObject(source)) => Some(source.as_str()),
            _ => None,
        }
    }

    pub(crate) fn captures_default(&self) -> bool {
        self.captures_default
    }
}

/// Emit the public prop shape without moving runtime values into type position.
pub(super) fn emit_options_api_props_type(
    ts: &mut String,
    generic_decl: &str,
    source: &OptionsApiPropsSource,
    type_name: &str,
    exported: bool,
) {
    if source.captures_default {
        ts.push_str(
            "type __VizeDefaultProps<T> = __VizeIsAny<T> extends true ? {} : T extends abstract new (...args: any[]) => { $props: infer P } ? P : {};\n",
        );
    }
    match source.direct.as_ref() {
        Some(DirectOptionsApiPropsSource::DeferredObject(_)) => ts.push_str(
            "type __VizeOptionsPropRequired<T> = T extends { required: true } ? true : false;\ntype __VizeOptionsPropShape<T extends Record<string, any>> = { [K in keyof T as __VizeOptionsPropRequired<T[K]> extends true ? K : never]-?: __RuntimePropCtor<T[K]>; } & { [K in keyof T as __VizeOptionsPropRequired<T[K]> extends true ? never : K]?: __RuntimePropCtor<T[K]>; };\n",
        ),
        Some(DirectOptionsApiPropsSource::Names(names)) => {
            if exported {
                append!(*ts, "export type {type_name}{generic_decl} = {{\n");
            } else {
                append!(*ts, "type {type_name} = {{\n");
            }
            for name in names {
                append!(*ts, "  \"{name}\"?: unknown;\n");
            }
            ts.push('}');
            append_default_props(ts, source);
            ts.push_str(";\n");
        }
        None => {
            if exported {
                append!(
                    *ts,
                    "export type {type_name}{generic_decl} = __VizeDefaultProps<Awaited<ReturnType<typeof __setup>>[\"__default__\"]>;\n"
                );
            } else {
                append!(
                    *ts,
                    "type {type_name} = __VizeDefaultProps<Awaited<ReturnType<typeof __setup>>[\"__default__\"]>;\n"
                );
            }
        }
    }
}

pub(crate) fn append_default_props(ts: &mut String, source: &OptionsApiPropsSource) {
    if source.captures_default {
        ts.push_str(" & __VizeDefaultProps<Awaited<ReturnType<typeof __setup>>[\"__default__\"]>");
    }
}
