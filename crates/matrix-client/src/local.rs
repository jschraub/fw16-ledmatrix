//! User-local IPC only. Explicit overrides are for isolated development instances.
use std::io;

#[cfg(unix)]
#[path = "local/unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "local/windows.rs"]
mod platform;

pub use platform::{Listener, connect, default_endpoint};

pub trait LocalStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> LocalStream for T {}
pub type Stream = Box<dyn LocalStream>;

pub fn endpoint() -> io::Result<String> {
    std::env::var("MATRIX_ENDPOINT").map_or_else(|_| default_endpoint(), Ok)
}
