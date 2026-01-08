//! Transform plugins for Vue template AST.
//!
//! This module contains individual transform plugins that process specific
//! directives and node types during the transform phase.

pub mod hoist_static;
pub mod transform_element;
pub mod transform_expression;
pub mod transform_text;
pub mod v_bind;
pub mod v_for;
pub mod v_if;
pub mod v_memo;
pub mod v_model;
pub mod v_on;
pub mod v_once;
pub mod v_slot;

pub use hoist_static::*;
pub use transform_element::*;
pub use transform_expression::{
    is_simple_identifier, prefix_identifiers_in_expression, process_expression,
    process_inline_handler, strip_typescript_from_expression,
};
pub use transform_text::*;
pub use v_bind::*;
pub use v_for::*;
pub use v_if::*;
pub use v_memo::*;
pub use v_model::*;
pub use v_on::*;
pub use v_once::*;
pub use v_slot::*;
