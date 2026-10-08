//! Bound the SDK's newline-delimited JSON reader without maintaining a JSON-RPC parser.
use rmcp::{
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{async_rw::AsyncRwTransport, Transport},
    RoleClient,
};
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    process::{Child, ChildStdin, ChildStdout, Command},
};

const LINE_LIMIT: usize = 4 * 1024 * 1024;
pub struct BoundedRead<R> {
    inner: R,
    line_bytes: usize,
}
impl<R> BoundedRead<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            line_bytes: 0,
        }
    }
}
impl<R: AsyncRead + Unpin> AsyncRead for BoundedRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut bytes = [0; 8192];
        let capacity = dst.remaining().min(bytes.len());
        let mut buffer = ReadBuf::new(&mut bytes[..capacity]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut buffer) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Ready(Ok(())) => {}
        }
        for byte in buffer.filled() {
            self.line_bytes += 1;
            if self.line_bytes > LINE_LIMIT {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stdio message exceeds 4 MiB",
                )));
            }
            if *byte == b'\n' {
                self.line_bytes = 0;
            }
        }
        dst.put_slice(buffer.filled());
        Poll::Ready(Ok(()))
    }
}
pub struct ChildTransport {
    child: Option<Child>,
    transport: AsyncRwTransport<RoleClient, BoundedRead<ChildStdout>, ChildStdin>,
}
pub fn spawn(mut command: Command) -> io::Result<ChildTransport> {
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing stdout"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing stdin"))?;
    Ok(ChildTransport {
        child: Some(child),
        transport: AsyncRwTransport::new(BoundedRead::new(stdout), stdin),
    })
}
impl Transport<RoleClient> for ChildTransport {
    type Error = io::Error;
    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        self.transport.send(message)
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.transport.receive().await
    }
    async fn close(&mut self) -> io::Result<()> {
        self.transport.close().await?;
        if let Some(mut child) = self.child.take() {
            if tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .is_err()
            {
                child.kill().await?;
            }
        }
        Ok(())
    }
}
impl Drop for ChildTransport {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // kill() waits/reaps, including when startup or a request future is cancelled.
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    #[tokio::test]
    async fn bounded_before_json_allocation_and_resets_per_line() {
        let bytes = vec![b'x'; LINE_LIMIT + 1];
        assert!(BoundedRead::new(bytes.as_slice())
            .read_to_end(&mut Vec::new())
            .await
            .is_err());
        let mut bytes = vec![b'x'; LINE_LIMIT - 1];
        bytes.push(b'\n');
        bytes.extend(vec![b'x'; LINE_LIMIT - 1]);
        bytes.push(b'\n');
        assert_eq!(
            BoundedRead::new(bytes.as_slice())
                .read_to_end(&mut Vec::new())
                .await
                .unwrap(),
            LINE_LIMIT * 2
        );
    }
}
