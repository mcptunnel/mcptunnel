//! Test provider: announces a fake origin and records its isolated ingress/PID.
//! Never connects to Cloudflare; used only by scripts/smoke-routing.mjs.
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let ingress = args
        .windows(2)
        .find(|pair| pair[0] == "--url")
        .map(|pair| &pair[1])
        .ok_or_else(|| anyhow::anyhow!("missing --url"))?;
    let port = url::Url::parse(ingress)?.port().unwrap();
    let origin = format!("https://mock-{port}.trycloudflare.com");
    let log = std::env::var("MCPTUNNEL_TEST_PROVIDER_EVENTS")?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    writeln!(
        log,
        "{}",
        serde_json::json!({"pid":std::process::id(), "ingress":ingress, "origin":origin})
    )?;
    eprintln!("{origin}");
    loop {
        std::thread::park();
    }
}
