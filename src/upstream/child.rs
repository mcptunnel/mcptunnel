use crate::mount::types::CommandConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RunningService, RoleClient, ServiceExt, Peer};
use rmcp::transport::TokioChildProcess;
use tokio::process::Command;
use tracing::{error, info};

/// A running upstream MCP child process session.
pub struct UpstreamChild {
    client: RunningService<RoleClient, ()>,
    pid: Option<u32>,
}

impl UpstreamChild {
    /// Spawn a new child process and initialize MCP.
    pub async fn spawn(
        config: &CommandConfig,
        cwd: Option<&str>,
        env_vars: &[(String, String)],
    ) -> Result<Self, crate::error::McpTunnelError> {
        let mut cmd = Command::new(&config.program);
        cmd.args(&config.args);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| crate::error::McpTunnelError::UpstreamSpawnFailed(e.to_string()))?;

        let pid = None; // PID is set by rmcp internally

        let client = ().serve(transport)
            .await
            .map_err(|e| crate::error::McpTunnelError::UpstreamSpawnFailed(
                format!("MCP initialization failed: {e}")
            ))?;

        info!(program = %config.program, "Upstream child process initialized");

        Ok(Self { client, pid })
    }

    /// Get the peer for making MCP requests.
    pub fn peer(&self) -> &Peer<RoleClient> {
        self.client.peer()
    }

    /// Send a JSON-RPC request to the upstream and get a raw JSON response.
    /// This handles arbitrary MCP methods by working at the JSON level.
    pub async fn send_raw_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, crate::error::McpTunnelError> {
        // For known MCP methods, use the typed API
        match method {
            "tools/list" => {
                let tools = self.client.list_all_tools().await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                Ok(serde_json::json!({ "tools": tools }))
            }
            "tools/call" => {
                let call_params: CallToolRequestParams = params
                    .map(|p| serde_json::from_value(p))
                    .transpose()
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?
                    .ok_or_else(|| crate::error::McpTunnelError::InternalGatewayError(
                        "tools/call requires params".to_string()
                    ))?;
                let result = self.client.call_tool(call_params).await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                serde_json::to_value(result)
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))
            }
            "resources/list" => {
                let resources = self.client.list_all_resources().await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                Ok(serde_json::json!({ "resources": resources }))
            }
            "resources/read" => {
                let read_params = params
                    .map(|p| serde_json::from_value(p))
                    .transpose()
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?
                    .ok_or_else(|| crate::error::McpTunnelError::InternalGatewayError(
                        "resources/read requires params".to_string()
                    ))?;
                let result = self.client.read_resource(read_params).await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                serde_json::to_value(result)
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))
            }
            "prompts/list" => {
                let prompts = self.client.list_all_prompts().await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                Ok(serde_json::json!({ "prompts": prompts }))
            }
            "prompts/get" => {
                let get_params = params
                    .map(|p| serde_json::from_value(p))
                    .transpose()
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?
                    .ok_or_else(|| crate::error::McpTunnelError::InternalGatewayError(
                        "prompts/get requires params".to_string()
                    ))?;
                let result = self.client.get_prompt(get_params).await
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))?;
                serde_json::to_value(result)
                    .map_err(|e| crate::error::McpTunnelError::InternalGatewayError(e.to_string()))
            }
            _ => {
                Err(crate::error::McpTunnelError::InternalGatewayError(
                    format!("unsupported MCP method: {method}")
                ))
            }
        }
    }

    /// Check if the child process is still running.
    pub fn is_closed(&self) -> bool {
        self.client.is_closed()
    }

    /// Shut down the child process gracefully.
    pub async fn shutdown(self) {
        let _ = self.client.cancel().await;
    }
}
