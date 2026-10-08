use crate::mount::types::{CommandConfig, ProbeResult};
use crate::upstream::child::UpstreamChild;
use rmcp::model::RequestMetaObject;
use serde_json::json;

pub async fn probe_upstream(
    config: &CommandConfig,
    cwd: Option<&str>,
    env_vars: &[(String, String)],
) -> ProbeResult {
    let mut result = ProbeResult {
        ok: false,
        protocol_version: None,
        server_name: None,
        server_version: None,
        tools: vec![],
        tool_definitions: vec![],
        resources: vec![],
        prompts: vec![],
        capabilities: None,
        error: None,
        stderr_snippet: None,
    };
    let child = match UpstreamChild::spawn(config, cwd, env_vars).await {
        Ok(child) => child,
        Err(e) => {
            result.error = Some(e.to_string());
            return result;
        }
    };
    if let Some(info) = child.peer().peer_info() {
        result.server_name = info.server_info.as_ref().map(|s| s.name.clone());
        result.server_version = info.server_info.as_ref().map(|s| s.version.clone());
        result.protocol_version = Some(info.protocol_version.as_str().into());
        result.capabilities = serde_json::to_value(&info.capabilities).ok();
    }
    let ct = tokio_util::sync::CancellationToken::new();
    for (method, key, cap) in [
        ("tools/list", "tools", "tools"),
        ("resources/list", "resources", "resources"),
        ("prompts/list", "prompts", "prompts"),
    ] {
        if result
            .capabilities
            .as_ref()
            .and_then(|v| v.get(cap))
            .is_none()
        {
            continue;
        }
        let mut cursor = None;
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        let mut catalog_bytes = 0usize;
        loop {
            let params = cursor
                .as_ref()
                .map_or(json!({}), |cursor: &String| json!({"cursor":cursor}));
            let value = match child
                .request(method, params, RequestMetaObject::default(), &ct)
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    result.error = Some(format!("upstream advertised {cap} but {method} failed"));
                    child.shutdown().await;
                    return result;
                }
            };
            catalog_bytes += value.to_string().len();
            entries.extend(value[key].as_array().cloned().unwrap_or_default());
            cursor = value["nextCursor"].as_str().map(str::to_owned);
            if entries.len() > 4096
                || catalog_bytes > 16 * 1024 * 1024
                || cursor
                    .as_ref()
                    .is_some_and(|c| !seen.insert(c.clone()) || seen.len() >= 64)
            {
                result.error = Some("upstream discovery exceeds bounded catalog/pagination limits or repeats a cursor".into());
                child.shutdown().await;
                return result;
            }
            if cursor.is_none() {
                break;
            }
        }
        let names = entries
            .iter()
            .filter_map(|v| {
                v[if key == "resources" { "uri" } else { "name" }]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        match key {
            "tools" => {
                result.tools = names;
                result.tool_definitions = entries;
            }
            "resources" => result.resources = names,
            _ => result.prompts = names,
        }
    }
    result.ok = true;
    child.shutdown().await;
    result
}
