//! Type-aware LSP support for `.jsx`/`.tsx` Vue components (#1498).
//!
//! Today maestro surfaces JSX **compiler** diagnostics only (parse/lowering
//! errors) and generates no virtual TypeScript for JSX, so hover, completion,
//! go-to-definition, and type diagnostics don't work on `.jsx`/`.tsx`. This
//! module is the type-aware slice: it lowers a JSX/TSX document to plain
//! virtual TypeScript (matching the `vize check` type-checker — standing
//! directive that JSX virtual TS stays plain `.ts`, never a TSX-format virtual
//! document), maps editor positions into that virtual TS, and reuses the SFC
//! Corsa machinery to answer requests.
//!
//! The whole surface is gated on the opt-in `typeChecker.jsxTypecheck` flag
//! (default off) so React `.tsx` files are never type-checked as Vue JSX.

#[cfg(any(test, feature = "native"))]
mod position;
pub mod virtual_ts;

// Structural (parse-based) JSX/TSX LSP providers. These need no Corsa bridge —
// like their SFC counterparts they answer from the parsed/lowered document —
// so they are available regardless of the `native` feature and are NOT gated on
// `typeChecker.jsxTypecheck`.
mod code_action;
mod document_symbols;
mod scoped_style;
mod semantic_tokens;

pub use code_action::JsxCodeActionService;
pub use document_symbols::JsxDocumentSymbolsService;
pub use scoped_style::JsxScopedStyleService;
pub use semantic_tokens::JsxSemanticTokensService;

// Type-aware JSX/TSX LSP providers over the Corsa bridge. Gated on `native`
// (the bridge is a native-only feature) and reached only when
// `typeChecker.jsxTypecheck` is enabled.
#[cfg(feature = "native")]
mod implementation;
#[cfg(feature = "native")]
mod references;
#[cfg(feature = "native")]
mod rename;
#[cfg(feature = "native")]
mod service;
#[cfg(feature = "native")]
mod service_project;
#[cfg(all(test, feature = "native"))]
mod signature_help_trace;
#[cfg(feature = "native")]
mod type_definition;

#[cfg(feature = "native")]
pub use implementation::JsxImplementationService;
#[cfg(feature = "native")]
pub use references::JsxReferencesService;
#[cfg(feature = "native")]
pub use rename::JsxRenameService;
#[cfg(feature = "native")]
pub use service::JsxService;
#[cfg(all(test, feature = "native"))]
pub(in crate::ide) use signature_help_trace::signature_help_traced;
#[cfg(feature = "native")]
pub use type_definition::JsxTypeDefinitionService;
