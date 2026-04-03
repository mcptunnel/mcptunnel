use rand::Rng;

/// Word lists for generating memorable slugs.
const ADJECTIVES: &[&str] = &[
    "amber", "azure", "bold", "calm", "cool", "dark", "deep", "fair",
    "fast", "glad", "gold", "gray", "keen", "kind", "lean", "mild",
    "neat", "pale", "pure", "rare", "rich", "safe", "soft", "teal",
    "true", "warm", "wise", "wild", "bold", "firm",
];

const NOUNS: &[&str] = &[
    "arch", "beam", "bell", "bird", "bolt", "cape", "chip", "claw",
    "core", "crow", "dawn", "dock", "dove", "drum", "dust", "edge",
    "fern", "fire", "flux", "gate", "glow", "haze", "helm", "hive",
    "isle", "jade", "knot", "lamp", "lark", "leaf", "link", "loom",
    "moon", "nest", "node", "opal", "palm", "peak", "pine", "pond",
    "reed", "reef", "root", "ruby", "sage", "seal", "star", "tide",
    "vale", "vine", "wave", "wren", "zero",
];

/// Generate a memorable, URL-safe slug.
pub fn generate_slug() -> String {
    let mut rng = rand::thread_rng();
    let adj = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    let num: u16 = rng.gen_range(10..100);
    format!("{adj}-{noun}-{num}")
}

/// Validate that a slug is URL-safe and reasonable.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("slug cannot be empty".to_string());
    }
    if slug.len() > 64 {
        return Err("slug too long (max 64 chars)".to_string());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("slug must contain only alphanumeric, hyphens, or underscores".to_string());
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("slug must not start or end with a hyphen".to_string());
    }
    // Reserved slugs
    let reserved = ["healthz", "admin", "api", "static", "favicon.ico"];
    if reserved.contains(&slug) {
        return Err(format!("slug '{slug}' is reserved"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_slug() {
        let slug = generate_slug();
        assert!(validate_slug(&slug).is_ok());
        // Should match pattern: adjective-noun-NN
        let parts: Vec<&str> = slug.split('-').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_slug_uniqueness() {
        let slugs: Vec<String> = (0..100).map(|_| generate_slug()).collect();
        let unique: std::collections::HashSet<&String> = slugs.iter().collect();
        // Very unlikely to have collisions in 100 slugs
        assert!(unique.len() > 90);
    }

    #[test]
    fn test_validate_slug() {
        assert!(validate_slug("my-server").is_ok());
        assert!(validate_slug("painter").is_ok());
        assert!(validate_slug("my_server_2").is_ok());

        assert!(validate_slug("").is_err());
        assert!(validate_slug("-bad").is_err());
        assert!(validate_slug("bad-").is_err());
        assert!(validate_slug("has spaces").is_err());
        assert!(validate_slug("has/slash").is_err());
        assert!(validate_slug("healthz").is_err());
    }
}
