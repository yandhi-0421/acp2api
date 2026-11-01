#![allow(clippy::disallowed_methods)]
use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};

pub fn trace(event: &str, fields: impl std::fmt::Display) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LINES: AtomicUsize = AtomicUsize::new(0);
    if LINES.fetch_add(1, Ordering::Relaxed) >= 4096 {
        return;
    }
    let us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros();
    let mut line = format!(
        "[DEBUG-223] us={us} pid={} {event} {fields}",
        std::process::id()
    );
    while line.len() > 500 {
        line.pop();
    }
    eprintln!("{line}");
}

pub fn event(event: &str, fields: impl std::fmt::Display) {
    if std::env::var_os("ACP2API_DEBUG223").is_some() {
        trace(event, fields);
    }
}
pub struct Listener(pub TcpListener);
pub struct Socket {
    inner: TcpStream,
    peer: SocketAddr,
    pending: bool,
}
impl axum::serve::Listener for Listener {
    type Io = Socket;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Socket, SocketAddr) {
        let (inner, peer) = axum::serve::Listener::accept(&mut self.0).await;
        event(
            "http_socket_accept",
            format!("peer={peer} local={:?}", inner.local_addr()),
        );
        (
            Socket {
                inner,
                peer,
                pending: false,
            },
            peer,
        )
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }
}
impl AsyncRead for Socket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let capacity = buf.remaining();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        match &result {
            Poll::Pending if !self.pending => {
                self.pending = true;
                event("http_read_pending", format!("peer={}", self.peer));
            }
            Poll::Ready(Ok(())) => {
                self.pending = false;
                event(
                    "http_read_ready",
                    format!(
                        "peer={} n={} capacity={capacity}",
                        self.peer,
                        buf.filled().len() - before
                    ),
                );
            }
            Poll::Ready(Err(e)) => event(
                "http_read_error",
                format!(
                    "peer={} kind={:?} os={:?}",
                    self.peer,
                    e.kind(),
                    e.raw_os_error()
                ),
            ),
            _ => {}
        }
        result
    }
}
impl AsyncWrite for Socket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Err(e)) = &result {
            event(
                "http_write_error",
                format!(
                    "peer={} kind={:?} os={:?}",
                    self.peer,
                    e.kind(),
                    e.raw_os_error()
                ),
            );
        }
        result
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Err(e)) = &result {
            event(
                "http_write_error",
                format!(
                    "peer={} kind={:?} os={:?}",
                    self.peer,
                    e.kind(),
                    e.raw_os_error()
                ),
            );
        }
        result
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(cx);
        if let Poll::Ready(Err(e)) = &result {
            event(
                "http_flush_error",
                format!(
                    "peer={} kind={:?} os={:?}",
                    self.peer,
                    e.kind(),
                    e.raw_os_error()
                ),
            );
        }
        result
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
pub struct DropTrace(pub String);
impl Drop for DropTrace {
    fn drop(&mut self) {
        event("http_future_drop", &self.0);
    }
}
