#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn portable_daemon_ipc_and_fail_closed_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let state = temp.path().join("state");
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = socket.local_addr().unwrap();
    drop(socket);
    std::fs::write(
        &config,
        format!("[daemon]\nbind=\"{bind}\"\n[tunnel]\ndefault_mode=\"local\"\n"),
    )
    .unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mcptunnel"));
        command
            .env("MCPTUNNEL_CONFIG", &config)
            .env("MCPTUNNEL_STATE_DIR", &state);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        command
    };
    let mut daemon = Process(
        command()
            .args(["daemon", "start"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if command()
            .args(["daemon", "status", "--json"])
            .output()
            .unwrap()
            .status
            .success()
        {
            break;
        }
        assert!(Instant::now() < deadline, "daemon IPC never became ready");
        std::thread::sleep(Duration::from_millis(50));
    }
    let fixture = format!("{}/tests/fixtures/legacy.cjs", env!("CARGO_MANIFEST_DIR"));
    let created = command()
        .args([
            "run", "--slug", "test", "--tunnel", "local", "--auth", "basic", "--json", "--",
            "node", &fixture,
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    assert!(created["mount"]["public_url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
    assert!(created["mount"]["basic_credentials"]["password"].is_string());
    let inspected = command()
        .args(["inspect", "test", "--json"])
        .output()
        .unwrap();
    assert!(inspected.status.success());
    assert!(!String::from_utf8(inspected.stdout)
        .unwrap()
        .contains("password"));
    let doctor = command()
        .args(["doctor", "test", "--json"])
        .output()
        .unwrap();
    let doctor: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert!(doctor["daemon_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["check"] == "upstream_live" && c["status"] == "PASS"));
    for args in [
        vec![
            "run",
            "--tunnel",
            "managed",
            "--hostname",
            "stable.example",
            "--",
            "node",
            &fixture,
        ],
        vec![
            "run",
            "--tunnel",
            "local",
            "--hostname",
            "stable.example",
            "--",
            "node",
            &fixture,
        ],
        vec!["run", "--auth", "oauth", "--", "node", &fixture],
    ] {
        assert!(!command().args(args).output().unwrap().status.success());
    }
    assert!(command()
        .args(["stop", "test"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(command()
        .args(["daemon", "stop", "--force"])
        .output()
        .unwrap()
        .status
        .success());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if daemon.0.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "daemon failed to clean up on shutdown"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!state.join("daemon.sock").exists());
    std::fs::write(&config, "broken toml [").unwrap();
    assert!(!command()
        .args(["daemon", "start"])
        .output()
        .unwrap()
        .status
        .success());
}
