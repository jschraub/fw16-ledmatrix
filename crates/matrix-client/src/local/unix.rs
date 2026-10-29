use super::Stream;
use std::{
    fs::{File, OpenOptions, TryLockError},
    io,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

fn uid() -> u32 {
    // SAFETY: geteuid takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "local endpoint must be owned by this user and private",
    )
}

fn private_directory(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid() || metadata.mode() & 0o077 != 0 {
        return Err(denied());
    }
    Ok(())
}

fn private_socket(path: &Path) -> io::Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid() || metadata.mode() & 0o077 != 0
    {
        return Err(denied());
    }
    Ok(metadata)
}

fn same_user(stream: &tokio::net::UnixStream) -> io::Result<()> {
    if stream.peer_cred()?.uid() != uid() {
        return Err(denied());
    }
    Ok(())
}

pub fn default_endpoint() -> io::Result<String> {
    let root = dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .ok_or_else(|| io::Error::other("no user runtime/cache directory"))?;
    Ok(root
        .join("fw16-matrix")
        .join("daemon.sock")
        .to_string_lossy()
        .into_owned())
}

pub async fn connect(endpoint: &str) -> io::Result<Stream> {
    let path = Path::new(endpoint);
    private_directory(path.parent().ok_or_else(denied)?)?;
    private_socket(path)?;
    let stream = tokio::net::UnixStream::connect(path).await?;
    same_user(&stream)?;
    Ok(Box::new(stream))
}

pub struct Listener {
    inner: tokio::net::UnixListener,
    endpoint: PathBuf,
    identity: (u64, u64),
    // Keep the OS lock for the entire listener lifetime; never delete the lock file.
    _lock: File,
}

impl Listener {
    pub async fn bind(endpoint: &str) -> io::Result<Self> {
        let path = Path::new(endpoint);
        let parent = path.parent().ok_or_else(denied)?;
        if !parent.exists() {
            match std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
            {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        private_directory(parent)?;
        let filename = path.file_name().ok_or_else(denied)?;
        let mut lock_name = filename.to_os_string();
        lock_name.push(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent.join(lock_name))?;
        let metadata = lock.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != uid()
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(denied());
        }
        lock.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::AddrInUse,
                "daemon already running for this endpoint",
            ),
            TryLockError::Error(e) => e,
        })?;

        match private_socket(path) {
            Ok(_) => {
                // A locked stale socket can be recovered, but never replace a live
                // endpoint (including one owned by an older/incompatible daemon).
                match tokio::time::timeout(
                    Duration::from_millis(250),
                    tokio::net::UnixStream::connect(path),
                )
                .await
                {
                    Ok(Err(e)) if e.kind() == io::ErrorKind::ConnectionRefused => {
                        std::fs::remove_file(path)?
                    }
                    Ok(Err(e)) => return Err(e),
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "live endpoint already exists",
                        ));
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let inner = tokio::net::UnixListener::bind(path)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let metadata = private_socket(path)?;
        Ok(Self {
            inner,
            endpoint: path.into(),
            identity: (metadata.dev(), metadata.ino()),
            _lock: lock,
        })
    }

    pub async fn accept(&mut self) -> io::Result<Stream> {
        let (stream, _) = self.inner.accept().await?;
        same_user(&stream)?;
        Ok(Box::new(stream))
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.endpoint)
            && metadata.file_type().is_socket()
            && (metadata.dev(), metadata.ino()) == self.identity
        {
            let _ = std::fs::remove_file(&self.endpoint);
        }
    }
}
