//! vue/sfc-element-order
//!
//! Enforce a consistent order of top-level elements in SFC.
//!
//! This is Vize's implementation of `eslint-plugin-vue`'s `vue/block-order`,
//! and it carries that rule's default order — `[["script", "template"], "style"]`.
//! The nested group is what makes `<script>` and `<template>` interchangeable:
//! both orders are idiomatic (the official Vue docs and `create-vue` templates
//! put `<template>` first), so only `<style>` is pinned last. Enforcing a strict
//! `<script>` before `<template>` here would report a warning on the majority of
//! real Vue components that upstream accepts (#3223).
//!
//! 1. `<script>` / `<script setup>` and `<template>`, in either order
//! 2. `<style>`
//!
//! ## Examples
//!
//! ### Invalid
//! ```vue
//! <style>...</style>
//! <script setup>...</script>
//! ```
//!
//! ### Valid
//! ```vue
//! <script setup>...</script>
//! <template>...</template>
//! <style></style>
//! ```
//!
//! ```vue
//! <template>...</template>
//! <script setup>...</script>
//! <style></style>
//! ```

mod options;
#[cfg(test)]
mod tests;

use self::options::{CompiledSfcElementOrder, SfcElementType};
use crate::context::LintContext;
use crate::diagnostic::{LintDiagnostic, Severity};
use crate::rule::{Rule, RuleCategory, RuleMeta};
use vize_atelier_sfc::{BlockLocation, SfcParseOptions, parse_sfc};
use vize_s0::{String, cstr, profile};

pub use options::{SfcElementOrderGroup, SfcElementOrderOptions};

static META: RuleMeta = RuleMeta {
    name: "vue/sfc-element-order",
    description: "Enforce consistent order of SFC top-level elements",
    category: RuleCategory::Recommended,
    fixable: false,
    default_severity: Severity::Warning,
};

#[derive(Debug, Clone)]
struct OrderedBlock {
    label: String,
    rank: usize,
    start: u32,
    end: u32,
}

impl OrderedBlock {
    #[inline]
    fn new(label: String, rank: usize, loc: &BlockLocation) -> Self {
        Self {
            label,
            rank,
            start: loc.tag_start as u32,
            end: loc.tag_end as u32,
        }
    }
}

/// Enforce SFC element order.
#[derive(Default)]
pub struct SfcElementOrder {
    order: CompiledSfcElementOrder,
}

impl SfcElementOrder {
    pub fn new(options: SfcElementOrderOptions) -> Self {
        Self {
            order: CompiledSfcElementOrder::new(options),
        }
    }
}

impl Rule for SfcElementOrder {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn run_on_sfc<'a>(&self, ctx: &mut LintContext<'a>) {
        let owned_descriptor;
        let descriptor = if let Some(descriptor) = ctx.sfc_descriptor() {
            descriptor
        } else {
            owned_descriptor = match profile!(
                "patina.rule.sfc_element_order.parse_sfc",
                parse_sfc(
                    ctx.source,
                    SfcParseOptions {
                        filename: ctx.filename.into(),
                        ..Default::default()
                    },
                )
            ) {
                Ok(descriptor) => descriptor,
                Err(_) => return,
            };
            &owned_descriptor
        };

        let mut blocks =
            Vec::with_capacity(3 + descriptor.styles.len() + descriptor.custom_blocks.len());

        if let Some(script) = descriptor.script.as_ref() {
            self.push_block(&mut blocks, SfcElementType::Script, &script.loc);
        }
        if let Some(script_setup) = descriptor.script_setup.as_ref() {
            self.push_block(&mut blocks, SfcElementType::ScriptSetup, &script_setup.loc);
        }
        if let Some(template) = descriptor.template.as_ref() {
            self.push_block(&mut blocks, SfcElementType::Template, &template.loc);
        }
        for style in &descriptor.styles {
            self.push_block(&mut blocks, SfcElementType::Style, &style.loc);
        }
        for block in &descriptor.custom_blocks {
            self.push_block(
                &mut blocks,
                SfcElementType::Custom(block.block_type.as_ref()),
                &block.loc,
            );
        }

        blocks.sort_unstable_by_key(|block| block.start);

        // Upstream anchors each block against the first *earlier* block that
        // outranks it, not merely against its neighbour, so
        // `<style><script><template>` reports both the script and the template.
        // The block count is a handful, so the quadratic scan is free.
        for index in 1..blocks.len() {
            let current = &blocks[index];
            let Some(previous) = blocks[..index]
                .iter()
                .find(|block| block.rank > current.rank)
            else {
                continue;
            };

            ctx.report(
                LintDiagnostic::warn(
                    META.name,
                    cstr!(
                        "{} should come before {}",
                        current.label.as_str(),
                        previous.label.as_str()
                    ),
                    current.start,
                    current.end,
                )
                .with_help(self.order.help()),
            );
        }
    }
}

impl SfcElementOrder {
    fn push_block<'a>(
        &self,
        blocks: &mut Vec<OrderedBlock>,
        kind: SfcElementType<'a>,
        loc: &BlockLocation,
    ) {
        if let Some(rank) = self.order.rank_for(kind) {
            blocks.push(OrderedBlock::new(kind.label(), rank, loc));
        }
    }
}
