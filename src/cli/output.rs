use serde_json::Value;

/// Print human-readable mount list table.
pub fn print_mount_list(mounts: &[Value]) {
    if mounts.is_empty() {
        println!("No mounts registered.");
        return;
    }

    // Header
    println!(
        "{:<16} {:<10} {:<8} {:<8} {:<4} {:<5} {}",
        "SLUG", "STATUS", "TUNNEL", "AUTH", "SESS", "TOOLS", "URL"
    );
    println!("{}", "-".repeat(90));

    for mount in mounts {
        println!(
            "{:<16} {:<10} {:<8} {:<8} {:<4} {:<5} {}",
            mount["slug"].as_str().unwrap_or("?"),
            mount["status"].as_str().unwrap_or("?"),
            mount["tunnel"].as_str().unwrap_or("?"),
            mount["auth"].as_str().unwrap_or("?"),
            mount["sessions"].as_u64().unwrap_or(0),
            mount["tools"].as_u64().unwrap_or(0),
            mount["url"].as_str().unwrap_or("?"),
        );
    }
}

/// Print human-readable mount inspection.
pub fn print_mount_inspect(data: &Value) {
    println!("Mount: {}", data["slug"].as_str().unwrap_or("?"));
    println!("  Display name:   {}", data["display_name"].as_str().unwrap_or("?"));
    println!("  Status:         {}", data["status"].as_str().unwrap_or("?"));
    println!("  Auth mode:      {}", data["auth_mode"].as_str().unwrap_or("?"));
    println!("  Tunnel mode:    {}", data["tunnel_mode"].as_str().unwrap_or("?"));
    println!("  Command:        {}", data["command"].as_str().unwrap_or("?"));
    if let Some(creds_url) = data["credentials_url"].as_str() {
        println!("  MCP URL:        {}", creds_url);
    } else {
        println!("  Public URL:     {}", data["public_url"].as_str().unwrap_or("?"));
    }
    println!("  Sessions:       {}", data["active_sessions"].as_u64().unwrap_or(0));
    println!("  Created:        {}", data["created_at"].as_str().unwrap_or("?"));

    if let Some(probe) = data.get("last_probe") {
        if !probe.is_null() {
            println!("  Last probe:");
            if let Some(name) = probe.get("server_name").and_then(|n| n.as_str()) {
                println!("    Server:       {}", name);
            }
            if let Some(tools) = probe.get("tools").and_then(|t| t.as_array()) {
                let names: Vec<&str> = tools.iter()
                    .filter_map(|t| t.as_str())
                    .collect();
                println!("    Tools:        {}", names.join(", "));
            }
        }
    }

    if let Some(policy) = data.get("policy") {
        if !policy.is_null() {
            println!("  Policy:");
            if let Some(allow) = policy.get("allow_tools").and_then(|a| a.as_array()) {
                if !allow.is_empty() {
                    let names: Vec<&str> = allow.iter().filter_map(|t| t.as_str()).collect();
                    println!("    Allow tools:  {}", names.join(", "));
                }
            }
            if let Some(deny) = policy.get("deny_tools").and_then(|d| d.as_array()) {
                if !deny.is_empty() {
                    let names: Vec<&str> = deny.iter().filter_map(|t| t.as_str()).collect();
                    println!("    Deny tools:   {}", names.join(", "));
                }
            }
        }
    }
}

/// Print human-readable daemon status.
pub fn print_daemon_status(data: &Value) {
    println!("Daemon status:");
    println!("  Instance ID:    {}", data["instance_id"].as_str().unwrap_or("?"));
    println!("  Bind:           {}", data["bind"].as_str().unwrap_or("?"));
    println!("  Mounts:         {}", data["mounts"].as_u64().unwrap_or(0));
    println!("  Tunnel origin:  {}", data["tunnel_origin"].as_str().unwrap_or("not active"));
    println!("  Tunnel mode:    {}", data["tunnel_mode"].as_str().unwrap_or("?"));
}

/// Print human-readable doctor checks.
pub fn print_doctor_checks(checks: &[Value]) {
    for check in checks {
        let status = check["status"].as_str().unwrap_or("?");
        let detail = check["detail"].as_str().unwrap_or("?");
        println!("{:>4}  {}", status, detail);
    }
}

/// Print mount creation result.
pub fn print_mount_created(data: &Value, warnings: &[String]) {
    if let Some(mount) = data.get("mount") {
        println!("Mount created: {}", mount["slug"].as_str().unwrap_or("?"));
        println!("Tunnel mode: {}", mount["tunnel_mode"].as_str().unwrap_or("?"));
        println!("Auth mode: {}", mount["auth_mode"].as_str().unwrap_or("?"));

        // Show credentials URL if Basic Auth, otherwise plain URL
        if let Some(creds_url) = mount["credentials_url"].as_str() {
            println!("MCP URL (with credentials):");
            println!("{}", creds_url);
        } else {
            println!("Public MCP URL:");
            println!("{}", mount["public_url"].as_str().unwrap_or("?"));
        }

        // Show OAuth credentials if built-in OAuth mode
        if let Some(oauth_creds) = mount.get("oauth_credentials") {
            if !oauth_creds.is_null() {
                println!("\nOAuth login credentials:");
                println!("  Username: {}", oauth_creds["username"].as_str().unwrap_or("?"));
                println!("  Password: {}", oauth_creds["password"].as_str().unwrap_or("?"));
            }
        }
    }

    if !warnings.is_empty() {
        println!("\nWarnings:");
        for w in warnings {
            println!("- {w}");
        }
    }
}
