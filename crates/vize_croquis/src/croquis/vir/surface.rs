use std::fmt::Write;

use crate::macros::MacroKind;
use vize_carton::String;

use super::super::Croquis;

impl Croquis {
    pub(super) fn write_surface(&self, output: &mut String) {
        let has_surface = !self.macros.props().is_empty()
            || !self.macros.emits().is_empty()
            || !self.macros.models().is_empty()
            || self
                .macros
                .all_calls()
                .iter()
                .any(|c| matches!(c.kind, MacroKind::DefineExpose | MacroKind::DefineSlots));

        if !has_surface {
            return;
        }

        // [surface.props]
        if !self.macros.props().is_empty() {
            writeln!(output, "[surface.props]").ok();
            for prop in self.macros.props() {
                let req = if prop.required { "!" } else { "?" };
                let def = if prop.default_value.is_some() {
                    "="
                } else {
                    ""
                };
                if let Some(ref ty) = prop.prop_type {
                    writeln!(output, "{}{}:{}{}", prop.name, req, ty, def).ok();
                } else {
                    writeln!(output, "{}{}{}", prop.name, req, def).ok();
                }
            }
            writeln!(output).ok();
        }

        // [surface.emits]
        if !self.macros.emits().is_empty() {
            writeln!(output, "[surface.emits]").ok();
            for emit in self.macros.emits() {
                if let Some(ref ty) = emit.payload_type {
                    writeln!(output, "{}:{}", emit.name, ty).ok();
                } else {
                    writeln!(output, "{}", emit.name).ok();
                }
            }
            writeln!(output).ok();
        }

        // [surface.models]
        if !self.macros.models().is_empty() {
            writeln!(output, "[surface.models]").ok();
            for model in self.macros.models() {
                let name = if model.name.is_empty() {
                    "modelValue"
                } else {
                    model.name.as_str()
                };
                if let Some(ty) = model.model_type.as_ref() {
                    writeln!(output, "{}:{}", name, ty).ok();
                } else {
                    writeln!(output, "{}", name).ok();
                }
            }
            writeln!(output).ok();
        }

        // [surface.expose]
        let expose_calls: Vec<_> = self
            .macros
            .all_calls()
            .iter()
            .filter(|c| c.kind == MacroKind::DefineExpose)
            .collect();
        if !expose_calls.is_empty() {
            writeln!(output, "[surface.expose]").ok();
            for call in &expose_calls {
                if let Some(args) = &call.runtime_args {
                    writeln!(output, "{}", args).ok();
                } else {
                    writeln!(output, "@{}:{}", call.start, call.end).ok();
                }
            }
            writeln!(output).ok();
        }

        // [surface.slots]
        let slots_calls: Vec<_> = self
            .macros
            .all_calls()
            .iter()
            .filter(|c| c.kind == MacroKind::DefineSlots)
            .collect();
        if !slots_calls.is_empty() {
            writeln!(output, "[surface.slots]").ok();
            for call in &slots_calls {
                if let Some(type_args) = &call.type_args {
                    writeln!(output, "{}", type_args).ok();
                } else {
                    writeln!(output, "@{}:{}", call.start, call.end).ok();
                }
            }
            writeln!(output).ok();
        }
    }
}
