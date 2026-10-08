//! Bound the SDK's newline-delimited JSON reader without maintaining a JSON-RPC parser.
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use rmcp::{
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{async_rw::AsyncRwTransport, Transport},
    RoleClient,
};
use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    process::{ChildStdin, ChildStdout, Command},
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
    child: Option<Box<dyn ChildWrapper>>,
    transport: AsyncRwTransport<RoleClient, BoundedRead<ChildStdout>, ChildStdin>,
    pub(crate) cancellation_sent: Arc<AtomicBool>,
}
pub fn spawn(mut command: Command) -> io::Result<ChildTransport> {
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut command = CommandWrap::from(command);
    command.wrap(KillOnDrop);
    #[cfg(unix)]
    command.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    {
        let mut flags = process_wrap::tokio::CreationFlags(Default::default());
        flags.0 .0 = 0x08000000;
        command.wrap(flags).wrap(process_wrap::tokio::JobObject);
    }
    let mut child = command.spawn()?;
    let stdout = child
        .inner_mut()
        .stdout()
        .take()
        .ok_or_else(|| io::Error::other("missing stdout"))?;
    let stdin = child
        .inner_mut()
        .stdin()
        .take()
        .ok_or_else(|| io::Error::other("missing stdin"))?;
    Ok(ChildTransport {
        child: Some(child),
        transport: AsyncRwTransport::new(BoundedRead::new(stdout), stdin),
        cancellation_sent: Arc::new(AtomicBool::new(false)),
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
            // Cancellation is a notification with no acknowledgement. Closing stdin
            // drains its preceding bytes; allow a bounded interval for the peer to
            // consume them before forcing termination. Completed requests need no grace.
            let reaped = self.cancellation_sent.load(Ordering::Acquire)
                && matches!(
                    tokio::time::timeout(Duration::from_secs(1), child.wait()).await,
                    Ok(Ok(_))
                );
            // Requests are finished: do not delay their HTTP response waiting for a
            // server's background timers. Kill the whole group/job, then reap the launcher.
            // This also covers descendants that survive a launcher's earlier exit.
            let _ = child.start_kill();
            if !reaped {
                child.wait().await?;
            }
        }
        Ok(())
    }
}
impl Drop for ChildTransport {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            // kill() waits/reaps, including when startup or a request future is cancelled.
            tokio::spawn(async move {
                let _ = Box::into_pin(child.kill()).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn cleanup_does_not_wait_for_upstream_background_timers() {
        let mut command = Command::new("node");
        command.args(["-e", "process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:1,result:{}})+'\\n'); setInterval(()=>{},1000)"]);
        let mut transport = spawn(command).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), transport.receive())
                .await
                .unwrap()
                .is_some()
        );
        let started = std::time::Instant::now();
        transport.close().await.unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "completed requests must not wait for an upstream's background timers"
        );
    }
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

    #[tokio::test]
    async fn closes_descendants_after_the_launcher_exits() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("descendant.pid");
        let mut command = Command::new("node");
        command.args(["-e","const c=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});require('node:fs').writeFileSync(process.argv[1],String(c.pid));process.exit(0);"]).arg(&file);
        let mut child = spawn(command).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !file.exists() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(file)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), child.child.as_mut().unwrap().wait())
            .await
            .unwrap()
            .unwrap();
        child.close().await.unwrap();
        tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                let mut check=Command::new("node");check.args(["-e","try{process.kill(Number(process.argv[1]),0);process.exit(1)}catch{process.exit(0)}"]).arg(pid.to_string()).kill_on_drop(true);
                #[cfg(windows)]check.creation_flags(0x08000000);
                if check.status().await.unwrap().success(){break;}
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await.expect("upstream descendant survived cleanup");
    }
}
