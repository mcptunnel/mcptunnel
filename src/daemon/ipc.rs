//! Same-user IPC: permission-restricted Unix socket or local Windows named pipe.
#[cfg(unix)]
pub type Client = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub type Client = std::fs::File;

#[cfg(unix)]
pub fn connect() -> std::io::Result<Client> {
    Client::connect(crate::config::socket_path())
}
#[cfg(windows)]
pub fn connect() -> std::io::Result<Client> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name())
}

#[cfg(windows)]
fn pipe_name() -> String {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(
        crate::config::state_dir().to_string_lossy().as_bytes(),
    ));
    format!(r"\\.\pipe\mcptunnel-{}", &hash[..24])
}

#[cfg(unix)]
pub struct Listener(tokio::net::UnixListener);
#[cfg(windows)]
pub struct Listener(tokio::net::windows::named_pipe::NamedPipeServer);

impl Listener {
    #[cfg(unix)]
    pub fn bind() -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let path = crate::config::socket_path();
        // Refuse to overwrite a live endpoint.
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "daemon already running",
            ));
        }
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self(listener))
    }
    #[cfg(windows)]
    pub fn bind() -> std::io::Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(pipe_name())?;
        // Readiness marker only; never carries credentials. The pipe uses the creator's default
        // security descriptor: other users cannot open it for read/write admin requests.
        std::fs::write(crate::config::socket_path(), "named-pipe\n")?;
        Ok(Self(server))
    }
    #[cfg(unix)]
    pub async fn accept(&mut self) -> std::io::Result<tokio::net::UnixStream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
    #[cfg(windows)]
    pub async fn accept(
        &mut self,
    ) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        use tokio::net::windows::named_pipe::ServerOptions;
        self.0.connect().await?;
        let replacement = ServerOptions::new()
            .reject_remote_clients(true)
            .create(pipe_name())?;
        Ok(std::mem::replace(&mut self.0, replacement))
    }
}
