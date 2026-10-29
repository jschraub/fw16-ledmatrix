use std::io;

pub trait LocalStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> LocalStream for T {}
pub type Stream = Box<dyn LocalStream>;

/// Explicit endpoint overrides are useful for isolated tests and parallel dev checkouts.
pub fn default_endpoint() -> io::Result<String> {
    #[cfg(unix)]
    {
        let root = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .ok_or_else(|| io::Error::other("no user runtime/cache directory"))?;
        Ok(root
            .join("fw16-matrix")
            .join("daemon.sock")
            .to_string_lossy()
            .into_owned())
    }
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").map_err(io::Error::other)?;
        if !user
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ')
        {
            return Err(io::Error::other(
                "unsupported username; specify MATRIX_ENDPOINT",
            ));
        }
        Ok(format!(r"\\.\pipe\fw16-matrix-{user}"))
    }
}

pub fn endpoint() -> io::Result<String> {
    std::env::var("MATRIX_ENDPOINT").map_or_else(|_| default_endpoint(), Ok)
}

pub async fn connect(endpoint: &str) -> io::Result<Stream> {
    #[cfg(unix)]
    {
        Ok(Box::new(tokio::net::UnixStream::connect(endpoint).await?))
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        for _ in 0..100 {
            match ClientOptions::new().open(endpoint) {
                Ok(stream) => return Ok(Box::new(stream)),
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "daemon pipe busy"))
    }
}

pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    inner: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
    endpoint: String,
}

impl Listener {
    pub fn bind(endpoint: &str) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::{os::unix::fs::PermissionsExt, path::Path};
            let parent = Path::new(endpoint)
                .parent()
                .ok_or_else(|| io::Error::other("endpoint needs a parent directory"))?;
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
            // Never unlink an endpoint we did not create, including stale sockets.
            let inner = tokio::net::UnixListener::bind(endpoint)?;
            std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self {
                inner,
                endpoint: endpoint.into(),
            })
        }
        #[cfg(windows)]
        {
            let inner = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(endpoint)?;
            Ok(Self {
                inner: Some(inner),
                endpoint: endpoint.into(),
            })
        }
    }

    pub async fn accept(&mut self) -> io::Result<Stream> {
        #[cfg(unix)]
        {
            Ok(Box::new(self.inner.accept().await?.0))
        }
        #[cfg(windows)]
        {
            let server = self
                .inner
                .as_ref()
                .expect("listener always has a pending pipe");
            server.connect().await?;
            let next = tokio::net::windows::named_pipe::ServerOptions::new()
                .reject_remote_clients(true)
                .create(&self.endpoint)?;
            Ok(Box::new(self.inner.replace(next).expect("pending pipe")))
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.endpoint);
        }
    }
}
