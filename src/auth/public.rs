use serde_json::json;

/// Inject `noauth` security scheme into tool descriptors.
pub fn inject_noauth_scheme(tools: &mut Vec<serde_json::Value>) {
    let noauth = json!([{"type": "noauth"}]);
    for tool in tools.iter_mut() {
        if let Some(obj) = tool.as_object_mut() {
            // Only inject if securitySchemes is not already present
            if !obj.contains_key("securitySchemes") {
                obj.insert("securitySchemes".to_string(), noauth.clone());
            }
            // Also mirror in _meta.securitySchemes for compatibility
            let meta = obj.entry("_meta").or_insert_with(|| json!({}));
            if let Some(meta_obj) = meta.as_object_mut() {
                if !meta_obj.contains_key("securitySchemes") {
                    meta_obj.insert("securitySchemes".to_string(), noauth.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_inject_noauth() {
        let mut tools = vec![
            json!({"name": "tool_a", "description": "A tool"}),
            json!({"name": "tool_b", "description": "B tool"}),
        ];
        inject_noauth_scheme(&mut tools);

        for tool in &tools {
            assert_eq!(tool["securitySchemes"], json!([{"type": "noauth"}]));
            assert_eq!(tool["_meta"]["securitySchemes"], json!([{"type": "noauth"}]));
        }
    }

    #[test]
    fn test_preserve_existing_scheme() {
        let mut tools = vec![
            json!({"name": "tool_a", "securitySchemes": [{"type": "oauth2"}]}),
        ];
        inject_noauth_scheme(&mut tools);

        // Should NOT overwrite existing securitySchemes
        assert_eq!(tools[0]["securitySchemes"], json!([{"type": "oauth2"}]));
    }
}
