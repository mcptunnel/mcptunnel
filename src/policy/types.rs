use serde::{Deserialize, Serialize};

/// Policy configuration for a mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    /// If non-empty, only these tools are exposed.
    pub allow_tools: Vec<String>,
    /// These tools are never exposed.
    pub deny_tools: Vec<String>,
    /// Whether to expose resources.
    pub expose_resources: bool,
    /// Whether to expose prompts.
    pub expose_prompts: bool,
    /// Allowed resource names (empty = all if expose_resources is true).
    pub allow_resources: Vec<String>,
    /// Denied resource names.
    pub deny_resources: Vec<String>,
    /// Allowed prompt names (empty = all if expose_prompts is true).
    pub allow_prompts: Vec<String>,
    /// Denied prompt names.
    pub deny_prompts: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allow_tools: vec![],
            deny_tools: vec![],
            expose_resources: true,
            expose_prompts: true,
            allow_resources: vec![],
            deny_resources: vec![],
            allow_prompts: vec![],
            deny_prompts: vec![],
        }
    }
}
