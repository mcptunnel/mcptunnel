use crate::mount::types::{CommandConfig, ProbeResult};
use rmcp::service::ServiceExt;
use rmcp::transport::TokioChildProcess;
use tokio::process::Command;
use tracing::{info, warn};

/// Probe an upstream MCP server to verify it speaks MCP and discover capabilities.
pub async fn probe_upstream(config: &CommandConfig, env_vars: &[(String, String)]) -> ProbeResult {
    info!(program = %config.program, "Probing upstream MCP server");

    let mut cmd = Command::new(&config.program);
    cmd.args(&config.args);
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // Try to spawn and initialize MCP
    let transport = match TokioChildProcess::new(cmd) {
        Ok(t) => t,
        Err(e) => {
            return ProbeResult {
                ok: false,
                protocol_version: None,
                server_name: None,
                server_version: None,
                tools: vec![],
                resources: vec![],
                prompts: vec![],
                capabilities: None,
                error: Some(format!("Failed to spawn child process: {e}")),
                stderr_snippet: None,
            };
        }
    };

    let client = match ().serve(transport).await {
        Ok(c) => c,
        Err(e) => {
            return ProbeResult {
                ok: false,
                protocol_version: None,
                server_name: None,
                server_version: None,
                tools: vec![],
                resources: vec![],
                prompts: vec![],
                capabilities: None,
                error: Some(format!("MCP initialization failed: {e}")),
                stderr_snippet: None,
            };
        }
    };

    // Extract server info
    let peer_info = client.peer_info();
    let server_name = peer_info.map(|i| i.server_info.name.clone());
    let server_version = peer_info.map(|i| i.server_info.version.clone());
    let protocol_version = peer_info.map(|i| i.protocol_version.as_str().to_string());
    let capabilities = peer_info.map(|i| {
        serde_json::to_value(&i.capabilities).unwrap_or(serde_json::Value::Null)
    });

    // List tools
    let tools: Vec<String> = match client.list_all_tools().await {
        Ok(tools) => tools.into_iter().map(|t| t.name.to_string()).collect(),
        Err(e) => {
            warn!(error = %e, "Failed to list tools during probe");
            vec![]
        }
    };

    // List resources
    let resources: Vec<String> = match client.list_all_resources().await {
        Ok(resources) => resources.into_iter().map(|r| r.uri.to_string()).collect(),
        Err(e) => {
            tracing::debug!(error = %e, "Failed to list resources during probe");
            vec![]
        }
    };

    // List prompts
    let prompts: Vec<String> = match client.list_all_prompts().await {
        Ok(prompts) => prompts.into_iter().map(|p| p.name.to_string()).collect(),
        Err(e) => {
            tracing::debug!(error = %e, "Failed to list prompts during probe");
            vec![]
        }
    };

    // Clean shutdown
    let _ = client.cancel().await;

    info!(
        server = server_name.as_deref().unwrap_or("unknown"),
        tools = tools.len(),
        resources = resources.len(),
        prompts = prompts.len(),
        "Probe successful"
    );

    ProbeResult {
        ok: true,
        protocol_version,
        server_name,
        server_version,
        tools,
        resources,
        prompts,
        capabilities,
        error: None,
        stderr_snippet: None,
    }
}
