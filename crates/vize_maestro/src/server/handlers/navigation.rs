use tower_lsp::lsp_types::request::{
    GotoDeclarationParams, GotoDeclarationResponse, GotoImplementationParams,
    GotoImplementationResponse, GotoTypeDefinitionParams, GotoTypeDefinitionResponse,
};
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{GotoDefinitionParams, GotoDefinitionResponse},
};

use super::super::MaestroServer;
#[cfg(feature = "native")]
use crate::ide::{
    DeclarationService, ImplementationService, JsxImplementationService, JsxService,
    JsxTypeDefinitionService, TypeDefinitionService,
};
use crate::ide::{DefinitionService, IdeContext, position_to_offset};

pub(super) type ImplParams = GotoImplementationParams;
pub(super) type ImplResponse = GotoImplementationResponse;
pub(super) type DefParams = GotoDefinitionParams;
pub(super) type DefResponse = GotoDefinitionResponse;
pub(super) type TypeDefParams = GotoTypeDefinitionParams;
pub(super) type TypeDefResponse = GotoTypeDefinitionResponse;
pub(super) type DeclParams = GotoDeclarationParams;
pub(super) type DeclResponse = GotoDeclarationResponse;

pub(super) async fn goto_definition(
    server: &MaestroServer,
    params: DefParams,
) -> Result<Option<DefResponse>> {
    if !server.state.lsp_features().definition {
        return Ok(None);
    }

    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let Some(content) = server.state.documents.text(uri) else {
        return Ok(None);
    };
    let Some(offset) = position_to_offset(&content, position.line, position.character) else {
        return Ok(None);
    };

    let ctx = IdeContext::with_content(&server.state, uri, offset, content);

    // Type-aware go-to-definition for `.jsx`/`.tsx` (opt-in
    // `typeChecker.jsxTypecheck`). React `.tsx` is untouched when off.
    #[cfg(feature = "native")]
    if crate::utils::is_jsx_path(uri.path()) {
        if server.state.jsx_typecheck_enabled() {
            let corsa_bridge = server.state.get_corsa_bridge().await;
            if let Some(response) = JsxService::definition(&ctx, corsa_bridge).await {
                return Ok(Some(response));
            }
        }
        return Ok(None);
    }

    #[cfg(feature = "native")]
    {
        let corsa_bridge = server.state.get_corsa_bridge().await;
        if let Some(response) = DefinitionService::definition_with_corsa(&ctx, corsa_bridge).await {
            return Ok(Some(response));
        }
    }

    #[cfg(not(feature = "native"))]
    if let Some(response) = DefinitionService::definition(&ctx) {
        return Ok(Some(response));
    }

    Ok(None)
}

pub(super) async fn goto_type_definition(
    server: &MaestroServer,
    params: TypeDefParams,
) -> Result<Option<TypeDefResponse>> {
    if !server.state.lsp_features().definition || !server.state.lsp_features().typecheck {
        return Ok(None);
    }

    #[cfg(feature = "native")]
    {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(content) = server.state.documents.text(uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&content, position.line, position.character) else {
            return Ok(None);
        };

        let ctx = IdeContext::with_content(&server.state, uri, offset, content);

        if crate::utils::is_jsx_path(uri.path()) {
            if server.state.jsx_typecheck_enabled() {
                let corsa_bridge = server.state.get_corsa_bridge().await;
                return Ok(JsxTypeDefinitionService::type_definition(&ctx, corsa_bridge).await);
            }
            return Ok(None);
        }

        let corsa_bridge = server.state.get_corsa_bridge().await;
        return Ok(TypeDefinitionService::type_definition_with_corsa(&ctx, corsa_bridge).await);
    }

    #[cfg(not(feature = "native"))]
    {
        let _ = params;
        Ok(None)
    }
}

pub(super) async fn goto_declaration(
    server: &MaestroServer,
    params: DeclParams,
) -> Result<Option<DeclResponse>> {
    if !server.state.lsp_features().definition {
        return Ok(None);
    }

    #[cfg(feature = "native")]
    {
        if !server.state.lsp_features().typecheck {
            return Ok(None);
        }

        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(content) = server.state.documents.text(uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&content, position.line, position.character) else {
            return Ok(None);
        };

        let ctx = IdeContext::with_content(&server.state, uri, offset, content);

        if crate::utils::is_jsx_path(uri.path()) {
            return Ok(None);
        }

        let corsa_bridge = server.state.get_corsa_bridge().await;
        return Ok(DeclarationService::declaration_with_corsa(&ctx, corsa_bridge).await);
    }

    #[cfg(not(feature = "native"))]
    {
        let _ = params;
        Ok(None)
    }
}

pub(super) async fn goto_implementation(
    server: &MaestroServer,
    params: GotoImplementationParams,
) -> Result<Option<GotoImplementationResponse>> {
    if !server.state.lsp_features().definition || !server.state.lsp_features().typecheck {
        return Ok(None);
    }

    #[cfg(feature = "native")]
    {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(content) = server.state.documents.text(uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&content, position.line, position.character) else {
            return Ok(None);
        };

        let ctx = IdeContext::with_content(&server.state, uri, offset, content);

        if crate::utils::is_jsx_path(uri.path()) {
            if server.state.jsx_typecheck_enabled() {
                let corsa_bridge = server.state.get_corsa_bridge().await;
                return Ok(JsxImplementationService::implementation(&ctx, corsa_bridge).await);
            }
            return Ok(None);
        }

        let corsa_bridge = server.state.get_corsa_bridge().await;
        return Ok(ImplementationService::implementation_with_corsa(&ctx, corsa_bridge).await);
    }

    #[cfg(not(feature = "native"))]
    {
        let _ = params;
        Ok(None)
    }
}
