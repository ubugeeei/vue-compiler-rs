//! Go-to-implementation for `.jsx`/`.tsx` Vue components over the Corsa bridge.

use std::sync::Arc;

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location};
use vize_canon::CorsaBridge;

use super::service::JsxService;
use crate::ide::{IdeContext, TypeDefinitionService};

/// Implementation navigation support for opt-in JSX/TSX virtual TypeScript.
pub struct JsxImplementationService;

impl JsxImplementationService {
    /// Go-to-implementation on a `.jsx`/`.tsx` component, resolved through
    /// virtual TS and mapped back to authored source.
    pub async fn implementation(
        ctx: &IdeContext<'_>,
        corsa_bridge: Option<Arc<CorsaBridge>>,
    ) -> Option<GotoDefinitionResponse> {
        let bridge = corsa_bridge?;
        let (virtual_ts, request_uri, line, character) =
            JsxService::prepare_request(ctx, &bridge).await?;

        let locations = bridge
            .implementation(&request_uri, line, character)
            .await
            .ok()?;
        if locations.is_empty() {
            return None;
        }

        let mapped: Vec<Location> = locations
            .iter()
            .filter_map(|location| {
                JsxService::map_location(ctx, &virtual_ts, &request_uri, location)
            })
            .collect();

        TypeDefinitionService::convert_locations(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ServerState;
    use tower_lsp::lsp_types::Url;

    fn ctx_for<'a>(
        state: &'a ServerState,
        uri: &'a Url,
        source: &str,
        marker: &str,
    ) -> IdeContext<'a> {
        let offset = source.find(marker).expect("marker present") + marker.len();
        IdeContext::with_content(state, uri, offset, source.to_string())
    }

    #[test]
    fn implementation_without_bridge_returns_none() {
        crate::runtime::block_on(async {
            let source = "interface Box { render(): string }\nclass Card implements Box { render() { return 'x'; } }\n";
            let uri = Url::parse("file:///tmp/Comp.tsx").unwrap();
            let state = ServerState::new();
            let ctx = ctx_for(&state, &uri, source, "render");
            assert!(
                JsxImplementationService::implementation(&ctx, None)
                    .await
                    .is_none()
            );
        });
    }
}
