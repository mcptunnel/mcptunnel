use crate::{
    daemon::state::AppState, mount::types::MountRecord, policy::filter,
    upstream::child::UpstreamChild,
};
use rmcp::{model::*, service::RequestContext, ErrorData, RoleServer, ServerHandler};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{borrow::Cow, sync::Arc};

/// Each SDK dispatch gets a fresh handler. Only explicit legacy sessions reuse a child.
pub struct Bridge {
    pub state: Arc<AppState>,
    pub mount: MountRecord,
}

impl Bridge {
    async fn forward<T: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
        context: RequestContext<RoleServer>,
    ) -> Result<T, ErrorData> {
        let params = serde_json::to_value(params)
            .map_err(|_| ErrorData::invalid_params("invalid parameters", None))?;
        let modern = context
            .protocol_version()
            .is_some_and(|v| !v.has_initialize());
        let mut value = if modern {
            let child = UpstreamChild::spawn(
                &self.mount.command,
                self.mount.cwd.as_deref(),
                &self.mount.env_vars,
            )
            .await
            .map_err(|_| {
                ErrorData::internal_error("upstream startup failed; run mcptunnel doctor", None)
            })?;
            let result = child
                .request(method, params, context.meta, &context.ct)
                .await;
            child.shutdown().await;
            result?
        } else {
            let sid = context
                .extensions
                .get::<axum::http::request::Parts>()
                .and_then(|p| p.headers.get("mcp-session-id"))
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ErrorData::invalid_request("legacy session required", None))?;
            let child = self
                .state
                .upstream_children
                .read()
                .await
                .get(sid)
                .cloned()
                .ok_or_else(|| {
                    ErrorData::invalid_request("session expired; initialize again", None)
                })?;
            child
                .request(method, params, context.meta, &context.ct)
                .await?
        };
        if modern {
            value["resultType"] = json!("complete");
            if value.get("_meta").is_none() {
                value["_meta"] = json!({});
            }
            value["_meta"]["io.modelcontextprotocol/serverInfo"] =
                json!({"name":"mcptunnel","version":env!("CARGO_PKG_VERSION")});
        }
        serde_json::from_value(value).map_err(|_| {
            ErrorData::internal_error(
                "upstream result is incompatible with the requested operation",
                None,
            )
        })
    }

    fn denied() -> ErrorData {
        ErrorData::new(
            rmcp::model::ErrorCode(filter::POLICY_VIOLATION_CODE as i32),
            "operation is not exposed by policy",
            None,
        )
    }

    fn capabilities(&self) -> ServerCapabilities {
        let mut value = self
            .mount
            .last_probe_result
            .as_ref()
            .and_then(|p| p.capabilities.clone())
            .unwrap_or(json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.retain(|key, _| ["tools", "resources", "prompts"].contains(&key.as_str()));
            if !self.mount.policy.expose_resources {
                obj.remove("resources");
            }
            if !self.mount.policy.expose_prompts {
                obj.remove("prompts");
            }
            for cap in obj.values_mut() {
                *cap = json!({});
            }
        }
        serde_json::from_value(value).unwrap_or_default()
    }
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(self.capabilities())
            .with_server_info(Implementation::new("mcptunnel", env!("CARGO_PKG_VERSION")))
    }
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[
            ProtocolVersion::V_2026_07_28,
            ProtocolVersion::V_2025_11_25,
            ProtocolVersion::V_2025_06_18,
            ProtocolVersion::V_2025_03_26,
        ])
    }
    async fn list_tools(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result: Value = self
            .forward("tools/list", params.unwrap_or_default(), context)
            .await?;
        let tools = result
            .get_mut("tools")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ErrorData::internal_error("upstream tools/list lacks tools", None))?;
        tools.retain(|t| {
            t["name"]
                .as_str()
                .is_some_and(|n| filter::is_tool_allowed(&self.mount.policy, n))
        });
        tools.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        if self.mount.auth_mode == crate::config::AuthModeConfig::Public {
            crate::auth::public::inject_noauth_scheme(tools);
        }
        serde_json::from_value(result)
            .map_err(|_| ErrorData::internal_error("invalid upstream tool definitions", None))
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.mount
            .last_probe_result
            .as_ref()?
            .tool_definitions
            .iter()
            .find(|t| t["name"].as_str() == Some(name))
            .and_then(|t| serde_json::from_value(t.clone()).ok())
    }
    async fn call_tool(
        &self,
        params: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !filter::is_tool_allowed(&self.mount.policy, &params.name) {
            return Err(Self::denied());
        }
        if params.input_responses.is_some() || params.request_state.is_some() {
            return Err(ErrorData::invalid_params(
                "MRTR continuations are not supported by this bridge",
                None,
            ));
        }
        self.forward::<CallToolResult>("tools/call", params, context)
            .await
            .map(Into::into)
    }
    async fn list_resources(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        if !self.mount.policy.expose_resources {
            return Ok(ListResourcesResult::default());
        }
        let mut result: Value = self
            .forward("resources/list", params.unwrap_or_default(), context)
            .await?;
        if let Some(items) = result["resources"].as_array_mut() {
            items.retain(|v| {
                v["uri"]
                    .as_str()
                    .is_some_and(|s| filter::is_resource_allowed(&self.mount.policy, s))
            });
        }
        serde_json::from_value(result)
            .map_err(|_| ErrorData::internal_error("invalid resource list", None))
    }
    async fn list_resource_templates(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        if !self.mount.policy.expose_resources {
            return Ok(ListResourceTemplatesResult::default());
        }
        let mut result: Value = self
            .forward(
                "resources/templates/list",
                params.unwrap_or_default(),
                context,
            )
            .await?;
        if let Some(items) = result["resourceTemplates"].as_array_mut() {
            items.retain(|v| {
                v["uriTemplate"]
                    .as_str()
                    .is_some_and(|s| filter::is_resource_allowed(&self.mount.policy, s))
            });
        }
        serde_json::from_value(result)
            .map_err(|_| ErrorData::internal_error("invalid resource template list", None))
    }
    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if !filter::is_resource_allowed(&self.mount.policy, &params.uri) {
            return Err(Self::denied());
        }
        self.forward::<ReadResourceResult>("resources/read", params, context)
            .await
            .map(Into::into)
    }
    async fn list_prompts(
        &self,
        params: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        if !self.mount.policy.expose_prompts {
            return Ok(ListPromptsResult::default());
        }
        let mut result: Value = self
            .forward("prompts/list", params.unwrap_or_default(), context)
            .await?;
        if let Some(items) = result["prompts"].as_array_mut() {
            items.retain(|v| {
                v["name"]
                    .as_str()
                    .is_some_and(|s| filter::is_prompt_allowed(&self.mount.policy, s))
            });
        }
        serde_json::from_value(result)
            .map_err(|_| ErrorData::internal_error("invalid prompt list", None))
    }
    async fn get_prompt(
        &self,
        params: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        if !filter::is_prompt_allowed(&self.mount.policy, &params.name) {
            return Err(Self::denied());
        }
        self.forward::<GetPromptResult>("prompts/get", params, context)
            .await
            .map(Into::into)
    }
    async fn complete(
        &self,
        params: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        self.forward("completion/complete", params, context).await
    }
}
