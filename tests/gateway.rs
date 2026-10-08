use mcptunnel::{
    config::{AuthModeConfig, Config, TunnelMode},
    daemon::state::AppState,
    mount::{db, types::*, MountRegistry},
    policy::types::PolicyConfig,
    session::types::SessionConfig,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

struct Server {
    state: Arc<AppState>,
    url: String,
    client: reqwest::Client,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.state.cancellation.cancel();
        self.task.abort();
    }
}
async fn server(policy: PolicyConfig) -> Server {
    server_options(policy, TunnelMode::Quick, None, 8).await
}
async fn server_options(
    policy: PolicyConfig,
    mode: TunnelMode,
    events: Option<String>,
    inflight: usize,
) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind = listener.local_addr().unwrap().to_string();
    let mut config = Config::default();
    config.daemon.bind = bind.clone();
    config.tunnel.default_mode = mode;
    config.defaults.max_inflight_per_session = inflight;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    db::init_db(&conn).unwrap();
    let registry = MountRegistry::new(Arc::new(Mutex::new(conn)));
    for (slug, auth) in [
        ("test", AuthModeConfig::Public),
        ("other", AuthModeConfig::Public),
        ("private", AuthModeConfig::Basic),
    ] {
        let record = registry
            .create(MountCreateParams {
                slug: Some(slug.into()),
                display_name: None,
                command: CommandConfig {
                    program: "node".into(),
                    args: vec![format!(
                        "{}/tests/fixtures/legacy.cjs",
                        env!("CARGO_MANIFEST_DIR")
                    )],
                },
                cwd: None,
                tunnel_mode: mode,
                hostname: None,
                auth_mode: auth,
                basic_credentials: None,
                oauth_config: None,
                policy: policy.clone(),
                env_vars: events
                    .as_ref()
                    .map(|path| vec![("FIXTURE_EVENTS".into(), path.clone())])
                    .unwrap_or_default(),
            })
            .unwrap();
        let probe = mcptunnel::upstream::probe::probe_upstream(&record.command, None, &[]).await;
        assert!(
            probe.ok,
            "real stdio fixture must initialize: {:?}",
            probe.error
        );
        registry.set_probe_result(slug, &probe).unwrap();
    }
    let state = Arc::new(AppState::new(config, registry, SessionConfig::default()));
    let router = mcptunnel::gateway::build_router(state.clone());
    let stop = CancellationToken::new();
    let ct = stop.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(ct.cancelled_owned())
            .await
            .unwrap();
    });
    Server {
        state,
        url: format!("http://{bind}"),
        client: reqwest::Client::new(),
        stop,
        task,
    }
}
fn modern(method: &str, params: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"regression","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}});
    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
}
impl Server {
    fn post(&self, slug: &str, body: &Value) -> reqwest::RequestBuilder {
        let method = body["method"].as_str().unwrap();
        let mut r = self
            .client
            .post(format!("{}/{slug}/mcp", self.url))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2026-07-28")
            .header("mcp-method", method);
        if let Some(name) = body
            .pointer("/params/name")
            .or_else(|| body.pointer("/params/uri"))
            .and_then(Value::as_str)
        {
            r = r.header("mcp-name", name);
        }
        if let Some(text) = body
            .pointer("/params/arguments/text")
            .and_then(Value::as_str)
        {
            r = r.header("mcp-param-text", text);
        }
        r.json(body)
    }
    async fn initialize(&self, version: &str) -> String {
        let body = json!({"jsonrpc":"2.0","id":10,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"legacy","version":"1"}}});
        let response = self
            .client
            .post(format!("{}/test/mcp", self.url))
            .header("accept", "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let sid = response.headers()["mcp-session-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let reply: Value = response.json().await.unwrap();
        assert_eq!(reply["result"]["protocolVersion"], version);
        sid
    }
}

#[tokio::test]
async fn modern_calls_legacy_without_session_and_preserves_results() {
    let s = server(PolicyConfig::default()).await;
    let discover: Value = s
        .post("test", &modern("server/discover", json!({})))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(discover["result"]["supportedVersions"]
        .as_array()
        .unwrap_or_else(|| panic!("discovery reply: {discover}"))
        .contains(&json!("2026-07-28")));
    for method in [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ] {
        let r = s
            .post("test", &modern(method, json!({})))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert!(r.headers().get("mcp-session-id").is_none());
        let reply: Value = r.json().await.unwrap();
        assert_eq!(
            reply["result"]["resultType"], "complete",
            "{method}: {reply}"
        );
        assert_eq!(reply["result"]["cacheScope"], "private");
    }
    let reply: Value = s
        .post(
            "test",
            &modern(
                "tools/call",
                json!({"name":"echo","arguments":{"text":"hello"}}),
            ),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["value"], "hello");
    assert_eq!(reply["result"]["_meta"]["fixture"], "output");
    let resources: Value = s
        .post(
            "test",
            &modern("resources/read", json!({"uri":"test://resource"})),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        resources["result"]["contents"][0]["text"],
        "fixture resource"
    );
    let prompt: Value = s
        .post(
            "test",
            &modern(
                "prompts/get",
                json!({"name":"greet","arguments":{"name":"Ada"}}),
            ),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        prompt["result"]["messages"][0]["content"]["text"],
        "Hello Ada"
    );
    assert_eq!(s.state.session_manager.active_count("test").await, 0);
}

#[tokio::test]
async fn legacy_versions_sessions_isolation_and_notifications() {
    let s = server(PolicyConfig::default()).await;
    for version in ["2025-11-25", "2025-06-18", "2025-03-26"] {
        let sid = s.initialize(version).await;
        let notification = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let r = s
            .client
            .post(format!("{}/test/mcp", s.url))
            .header("mcp-session-id", &sid)
            .header("accept", "application/json, text/event-stream")
            .json(&notification)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 202);
        assert!(r.bytes().await.unwrap().is_empty());
        for expected in [1, 2] {
            let body =
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"counter"}});
            let r = s
                .client
                .post(format!("{}/test/mcp", s.url))
                .header("mcp-session-id", &sid)
                .header("mcp-protocol-version", version)
                .header("accept", "application/json, text/event-stream")
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
            let reply: Value = r.json().await.unwrap();
            assert_eq!(reply["result"]["structuredContent"]["value"], expected);
            let wrong = s
                .client
                .post(format!("{}/other/mcp", s.url))
                .header("mcp-session-id", &sid)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(wrong.status(), 404);
        }
        let r = s
            .client
            .delete(format!("{}/test/mcp", s.url))
            .header("mcp-session-id", &sid)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 204);
    }
}

#[tokio::test]
async fn protocol_headers_auth_policy_and_http_errors() {
    let policy = PolicyConfig {
        deny_tools: vec!["echo".into()],
        expose_resources: false,
        expose_prompts: false,
        ..Default::default()
    };
    let s = server(policy).await;
    let body = modern("tools/call", json!({"name":"echo"}));
    assert_eq!(s.post("private", &body).send().await.unwrap().status(), 401);
    let denied: Value = s
        .post("test", &body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(denied["error"]["code"], -32010);
    for body in [
        modern("resources/read", json!({"uri":"test://resource"})),
        modern("prompts/get", json!({"name":"greet"})),
    ] {
        let denied: Value = s
            .post("test", &body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(denied["error"]["code"], -32010);
    }
    let r = s
        .post("test", &modern("tools/list", json!({})))
        .header("mcp-method", "tools/call")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["error"]["code"], -32020);
    let mut body = modern("tools/list", json!({}));
    body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2099-01-01");
    let r = s
        .client
        .post(format!("{}/test/mcp", s.url))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-method", "tools/list")
        .header("mcp-protocol-version", "2099-01-01")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["error"]["code"], -32022);
    assert_eq!(
        s.post("test", &modern("tools/list", json!({})))
            .header("origin", "https://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        s.post("test", &modern("tools/list", json!({})))
            .header("host", "evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        s.client
            .get(format!("{}/test/mcp", s.url))
            .send()
            .await
            .unwrap()
            .status(),
        405
    );
    let r = s
        .post(
            "test",
            &modern("subscriptions/listen", json!({"notifications":{}})),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn concurrent_stateless_children_and_crash_recovery() {
    let s = server(PolicyConfig::default()).await;
    let bodies: Vec<_> = (0..4)
        .map(|_| modern("tools/call", json!({"name":"counter"})))
        .collect();
    let results = futures::future::join_all(bodies.iter().map(|b| async {
        s.post("test", b)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    }))
    .await;
    for r in results {
        assert_eq!(r["result"]["structuredContent"]["value"], 1);
    }
    let crash: Value = s
        .post("test", &modern("tools/call", json!({"name":"crash"})))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(crash.get("error").is_some());
    let recovery: Value = s
        .post("test", &modern("tools/call", json!({"name":"counter"})))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recovery["result"]["structuredContent"]["value"], 1);
    let error: Value = s
        .post("test", &modern("tools/call", json!({"name":"error"})))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(error["error"]["code"], -32602);
    assert_eq!(error["error"]["data"]["detail"], "preserved");
}

#[tokio::test]
async fn progress_sse_is_request_scoped_and_quick_remains_json() {
    for mode in [TunnelMode::Local, TunnelMode::Quick] {
        let s = server_options(PolicyConfig::default(), mode, None, 8).await;
        let mut body = modern("tools/call", json!({"name":"progress"}));
        body["params"]["_meta"]["progressToken"] = json!("remote-token");
        let response = s.post("test", &body).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let mime = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .to_owned();
        let text = response.text().await.unwrap();
        if mode == TunnelMode::Local {
            assert!(mime.starts_with("text/event-stream"));
            assert!(text.contains("notifications/progress"));
            assert!(text.contains("remote-token"));
            assert!(text.contains("progress done"));
            assert!(!text.contains("sampling/createMessage"));
        } else {
            assert!(mime.starts_with("application/json"));
            let reply: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(reply["result"]["content"][0]["text"], "progress done");
        }
    }
}

#[tokio::test]
async fn sse_disconnect_cancels_upstream_and_holds_admission_permits() {
    let temp = tempfile::tempdir().unwrap();
    let events = temp.path().join("events.txt");
    let s = server_options(
        PolicyConfig::default(),
        TunnelMode::Local,
        Some(events.to_string_lossy().into()),
        1,
    )
    .await;
    let mut body = modern("tools/call", json!({"name":"slow"}));
    body["params"]["_meta"]["progressToken"] = json!("cancel-token");
    let response = s.post("test", &body).send().await.unwrap();
    assert!(response.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    assert_eq!(
        s.post("test", &modern("tools/list", json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    drop(response);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if std::fs::read_to_string(&events)
                .unwrap_or_default()
                .contains("cancelled")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("disconnect must reach the actual stdio process");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if s.post("test", &modern("tools/list", json!({})))
                .send()
                .await
                .unwrap()
                .status()
                == 200
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("permit must be released after disconnect");
}

#[tokio::test]
async fn media_types_payloads_basic_and_modern_notifications() {
    let s = server(PolicyConfig::default()).await;
    let body = modern("tools/list", json!({}));
    let credentials = s
        .state
        .mount_registry
        .get("private")
        .unwrap()
        .unwrap()
        .basic_credentials
        .unwrap();
    assert_eq!(
        s.post("private", &body)
            .basic_auth(credentials.username, Some(credentials.password))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        s.post("private", &body)
            .header("authorization", "Basic invalid")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append("authorization", "Basic one".parse().unwrap());
    headers.append("authorization", "Basic two".parse().unwrap());
    assert_eq!(
        s.post("private", &body)
            .headers(headers)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        s.client
            .post(format!("{}/test/mcp", s.url))
            .header("accept", "text/plain")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        406
    );
    assert_eq!(
        s.client
            .post(format!("{}/test/mcp", s.url))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "text/plain")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        415
    );
    assert_eq!(
        s.client
            .post(format!("{}/test/mcp", s.url))
            .body("x".repeat(1024 * 1024 + 1))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    let mut notification = modern("notifications/cancelled", json!({"requestId":999}));
    notification.as_object_mut().unwrap().remove("id");
    let response = s.post("test", &notification).send().await.unwrap();
    assert_eq!(response.status(), 202);
    assert!(response.bytes().await.unwrap().is_empty());
    let response = s
        .client
        .post(format!("{}/test/mcp", s.url))
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        -32700
    );
}

#[tokio::test]
async fn timeout_cancels_a_real_nonresponding_child() {
    let temp = tempfile::tempdir().unwrap();
    let events = temp.path().join("events.txt");
    let s = server_options(
        PolicyConfig::default(),
        TunnelMode::Quick,
        Some(events.to_string_lossy().into()),
        8,
    )
    .await;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(75),
        s.post("test", &modern("tools/call", json!({"name":"hang"})))
            .send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), 200);
    let reply: Value = response.json().await.unwrap();
    assert_eq!(reply["error"]["code"], -32603);
    assert!(reply["error"]["message"]
        .as_str()
        .unwrap()
        .contains("timed out"));
    assert!(std::fs::read_to_string(events)
        .unwrap()
        .contains("cancelled"));
}

#[tokio::test]
async fn probe_collects_all_pages_and_rejects_repeated_cursors() {
    let command = CommandConfig {
        program: "node".into(),
        args: vec![format!(
            "{}/tests/fixtures/legacy.cjs",
            env!("CARGO_MANIFEST_DIR")
        )],
    };
    let probe = mcptunnel::upstream::probe::probe_upstream(
        &command,
        None,
        &[("FIXTURE_PAGED".into(), "1".into())],
    )
    .await;
    assert!(probe.ok);
    assert_eq!(probe.tools.len(), 7);
    assert_eq!(probe.tool_definitions.len(), 7);
    assert!(probe
        .tool_definitions
        .iter()
        .any(|t| t["name"] == "echo"
            && t["inputSchema"]["properties"]["text"]["x-mcp-header"] == "Text"));
    let bad = mcptunnel::upstream::probe::probe_upstream(
        &command,
        None,
        &[
            ("FIXTURE_PAGED".into(), "1".into()),
            ("FIXTURE_REPEAT_CURSOR".into(), "1".into()),
        ],
    )
    .await;
    assert!(!bad.ok);
    assert!(bad.error.unwrap().contains("repeats a cursor"));
}
