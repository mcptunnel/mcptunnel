use super::types::PolicyConfig;

/// JSON-RPC error code for policy violations.
pub const POLICY_VIOLATION_CODE: i64 = -32040;

/// Check if a tool name is allowed by the policy.
pub fn is_tool_allowed(policy: &PolicyConfig, tool_name: &str) -> bool {
    // Deny list takes precedence
    if policy.deny_tools.iter().any(|d| d == tool_name) {
        return false;
    }
    // If allow list is specified, tool must be in it
    if !policy.allow_tools.is_empty() {
        return policy.allow_tools.iter().any(|a| a == tool_name);
    }
    true
}

/// Check if a resource name is allowed by the policy.
pub fn is_resource_allowed(policy: &PolicyConfig, resource_name: &str) -> bool {
    if !policy.expose_resources {
        return false;
    }
    if policy.deny_resources.iter().any(|d| d == resource_name) {
        return false;
    }
    if !policy.allow_resources.is_empty() {
        return policy.allow_resources.iter().any(|a| a == resource_name);
    }
    true
}

/// Check if a prompt name is allowed by the policy.
pub fn is_prompt_allowed(policy: &PolicyConfig, prompt_name: &str) -> bool {
    if !policy.expose_prompts {
        return false;
    }
    if policy.deny_prompts.iter().any(|d| d == prompt_name) {
        return false;
    }
    if !policy.allow_prompts.is_empty() {
        return policy.allow_prompts.iter().any(|a| a == prompt_name);
    }
    true
}

/// Filter a list of tool descriptors, keeping only allowed ones.
pub fn filter_tools(policy: &PolicyConfig, tools: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    tools
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(|n| n.as_str())
                .map(|name| is_tool_allowed(policy, name))
                .unwrap_or(false)
        })
        .collect()
}

/// Filter a list of resource descriptors.
pub fn filter_resources(policy: &PolicyConfig, resources: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    if !policy.expose_resources {
        return vec![];
    }
    resources
        .into_iter()
        .filter(|resource| {
            resource.get("name")
                .or_else(|| resource.get("uri"))
                .and_then(|n| n.as_str())
                .map(|name| is_resource_allowed(policy, name))
                .unwrap_or(true) // pass through if no name field
        })
        .collect()
}

/// Filter a list of prompt descriptors.
pub fn filter_prompts(policy: &PolicyConfig, prompts: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    if !policy.expose_prompts {
        return vec![];
    }
    prompts
        .into_iter()
        .filter(|prompt| {
            prompt.get("name")
                .and_then(|n| n.as_str())
                .map(|name| is_prompt_allowed(policy, name))
                .unwrap_or(true)
        })
        .collect()
}

/// Suspicious tool name heuristics for doctor warnings.
pub fn flag_suspicious_tools(tool_names: &[String]) -> Vec<String> {
    let suspicious_patterns = [
        "exec", "shell", "bash", "delete", "write", "filesystem",
        "browser", "sql", "eval", "system", "cmd", "run_command",
    ];
    tool_names
        .iter()
        .filter(|name| {
            let lower = name.to_lowercase();
            suspicious_patterns.iter().any(|p| lower.contains(p))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all_by_default() {
        let policy = PolicyConfig::default();
        assert!(is_tool_allowed(&policy, "anything"));
        assert!(is_resource_allowed(&policy, "anything"));
        assert!(is_prompt_allowed(&policy, "anything"));
    }

    #[test]
    fn test_deny_tool() {
        let policy = PolicyConfig {
            deny_tools: vec!["dangerous".to_string()],
            ..Default::default()
        };
        assert!(!is_tool_allowed(&policy, "dangerous"));
        assert!(is_tool_allowed(&policy, "safe"));
    }

    #[test]
    fn test_allow_tool() {
        let policy = PolicyConfig {
            allow_tools: vec!["paint.draw".to_string(), "paint.fill".to_string()],
            ..Default::default()
        };
        assert!(is_tool_allowed(&policy, "paint.draw"));
        assert!(is_tool_allowed(&policy, "paint.fill"));
        assert!(!is_tool_allowed(&policy, "paint.delete"));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let policy = PolicyConfig {
            allow_tools: vec!["tool_a".to_string(), "tool_b".to_string()],
            deny_tools: vec!["tool_b".to_string()],
            ..Default::default()
        };
        assert!(is_tool_allowed(&policy, "tool_a"));
        assert!(!is_tool_allowed(&policy, "tool_b"));
    }

    #[test]
    fn test_hide_resources() {
        let policy = PolicyConfig {
            expose_resources: false,
            ..Default::default()
        };
        assert!(!is_resource_allowed(&policy, "anything"));
    }

    #[test]
    fn test_hide_prompts() {
        let policy = PolicyConfig {
            expose_prompts: false,
            ..Default::default()
        };
        assert!(!is_prompt_allowed(&policy, "anything"));
    }

    #[test]
    fn test_filter_tools() {
        let policy = PolicyConfig {
            allow_tools: vec!["keep_me".to_string()],
            ..Default::default()
        };
        let tools = vec![
            serde_json::json!({"name": "keep_me", "description": "good"}),
            serde_json::json!({"name": "remove_me", "description": "bad"}),
        ];
        let filtered = filter_tools(&policy, tools);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["name"], "keep_me");
    }

    #[test]
    fn test_suspicious_tools() {
        let tools = vec![
            "paint.draw".to_string(),
            "exec_command".to_string(),
            "read_file".to_string(),
            "bash_run".to_string(),
        ];
        let suspicious = flag_suspicious_tools(&tools);
        assert_eq!(suspicious.len(), 2);
        assert!(suspicious.contains(&"exec_command".to_string()));
        assert!(suspicious.contains(&"bash_run".to_string()));
    }
}
