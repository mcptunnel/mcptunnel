use super::types::CommandConfig;

/// Deliberately accept only unambiguous, literal URL paths.
pub fn validate_path(path: &str) -> Result<(), String> {
    if path.len() > 512 || !path.starts_with('/') || path == "/" {
        return Err("--path must start with / and contain 1–512 characters".into());
    }
    let segments: Vec<_> = path[1..].split('/').collect();
    if segments.iter().any(|s| {
        s.is_empty()
            || *s == "."
            || *s == ".."
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    }) {
        return Err("--path requires literal letters, digits, -, _ or .; empty/dot segments, escapes, queries and fragments are rejected".into());
    }
    if matches!(
        segments[0],
        "healthz" | ".well-known" | "_mcptunnel" | "admin" | "api" | "static" | "favicon.ico"
    ) {
        return Err("--path uses a reserved gateway prefix".into());
    }
    Ok(())
}

pub fn infer_name(command: &CommandConfig) -> String {
    let mut program = command.program.as_str();
    let mut args = command.args.as_slice();
    if basename(program).eq_ignore_ascii_case("cmd") {
        if let Some(index) = args.iter().position(|arg| arg.eq_ignore_ascii_case("/c")) {
            if let Some(next) = args.get(index + 1) {
                program = next;
                args = &args[index + 2..];
            }
        }
    }
    let launcher = basename(program);
    let package = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--package="))
        .or_else(|| {
            args.windows(2)
                .find(|pair| pair[0] == "--package")
                .map(|pair| pair[1].as_str())
        });
    let candidate = match launcher {
        "npx" | "uvx" => args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .map(String::as_str),
        "npm" => package,
        "node" | "python" | "python3" => args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .map(String::as_str),
        _ => None,
    }
    .unwrap_or(program);
    let candidate = candidate
        .trim_start_matches('@')
        .split('@')
        .next()
        .unwrap_or(candidate);
    let candidate = basename(candidate);
    let candidate = candidate
        .trim_start_matches("server-")
        .trim_start_matches("mcp-server-")
        .trim_start_matches("mcp_server_")
        .trim_end_matches("-mcp");
    let candidate = if candidate == "mcp" {
        // Scoped packages such as @playwright/mcp use the scope as their name.
        command
            .args
            .iter()
            .find_map(|arg| arg.strip_prefix('@').and_then(|s| s.split('/').next()))
            .unwrap_or("server")
    } else {
        candidate
    };
    let name: String = candidate
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-');
    let name = name.chars().take(48).collect::<String>();
    if name.is_empty() {
        "server".into()
    } else {
        name
    }
}

fn basename(value: &str) -> &str {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".cjs")
        .trim_end_matches(".mjs")
        .trim_end_matches(".js")
        .trim_end_matches(".py")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_follow_real_servers() {
        for (program, args, expected) in [
            (
                "npx",
                vec![
                    "-y",
                    "@modelcontextprotocol/server-filesystem@2026.8.31",
                    "./shared",
                ],
                "filesystem",
            ),
            ("uvx", vec!["mcp-server-git"], "git"),
            ("npx", vec!["@playwright/mcp@latest"], "playwright"),
            (
                "cmd",
                vec![
                    "/d",
                    "/c",
                    "npx",
                    "-y",
                    "@modelcontextprotocol/server-filesystem",
                ],
                "filesystem",
            ),
            ("python", vec!["-m", "mcp_server_git"], "git"),
        ] {
            assert_eq!(
                infer_name(&CommandConfig {
                    program: program.into(),
                    args: args.into_iter().map(str::to_owned).collect()
                }),
                expected
            );
        }
    }
    #[test]
    fn paths_cannot_shadow_or_alias_routes() {
        for path in [
            "",
            "/",
            "files",
            "/files/",
            "/files//read",
            "/files/..",
            "/%66iles",
            "/a?b",
            "/a#b",
            "/healthz/x",
            "/.well-known/x",
            "/_mcptunnel/x",
            "/a\\b",
        ] {
            assert!(validate_path(path).is_err(), "{path}");
        }
        assert!(validate_path("/team/files-v2").is_ok());
    }
}
