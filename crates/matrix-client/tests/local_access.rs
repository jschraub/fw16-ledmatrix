use matrix_client::local;

fn private_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    directory
}

#[tokio::test]
async fn endpoint_is_exclusive_and_reusable_after_listener_exit() {
    let directory = private_directory();
    #[cfg(unix)]
    let endpoint = directory
        .path()
        .join("exclusive.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\matrix-access-{}-{}",
        std::process::id(),
        directory.path().file_name().unwrap().to_string_lossy()
    );
    let listener = local::Listener::bind(&endpoint).await.unwrap();
    assert!(local::Listener::bind(&endpoint).await.is_err());
    drop(listener);
    assert!(local::Listener::bind(&endpoint).await.is_ok());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    };

    #[tokio::test]
    async fn private_socket_and_directory_are_enforced_without_repairing_unsafe_paths() {
        let directory = private_directory();
        let parent = directory.path().join("private");
        let endpoint = parent.join("panel.sock");
        let listener = local::Listener::bind(endpoint.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&endpoint).unwrap().mode() & 0o777, 0o600);
        fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(
            local::connect(endpoint.to_str().unwrap())
                .await
                .err()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        drop(listener);
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            local::Listener::bind(endpoint.to_str().unwrap())
                .await
                .err()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::metadata(parent).unwrap().mode() & 0o777, 0o777);
    }

    #[tokio::test]
    async fn rejects_symlinks_and_regular_files_without_deleting_them() {
        let directory = private_directory();
        let target = directory.path().join("keep-me");
        fs::write(&target, "private contents").unwrap();
        let endpoint = directory.path().join("panel.sock");
        symlink(&target, &endpoint).unwrap();
        assert!(
            local::Listener::bind(endpoint.to_str().unwrap())
                .await
                .is_err()
        );
        assert!(fs::symlink_metadata(&endpoint).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(target).unwrap(), "private contents");
    }

    #[tokio::test]
    async fn recovers_stale_socket_but_never_replaces_a_live_unmanaged_endpoint() {
        let directory = private_directory();
        let endpoint = directory.path().join("panel.sock");
        let unmanaged = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
        fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            local::Listener::bind(endpoint.to_str().unwrap())
                .await
                .err()
                .unwrap()
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        assert!(endpoint.exists());
        drop(unmanaged);
        let listener = local::Listener::bind(endpoint.to_str().unwrap())
            .await
            .unwrap();
        drop(listener);
        assert!(!endpoint.exists());
    }

    #[tokio::test]
    async fn listener_cleanup_does_not_remove_a_replaced_path() {
        let directory = private_directory();
        let endpoint = directory.path().join("panel.sock");
        let listener = local::Listener::bind(endpoint.to_str().unwrap())
            .await
            .unwrap();
        fs::remove_file(&endpoint).unwrap();
        fs::write(&endpoint, "replacement").unwrap();
        drop(listener);
        assert_eq!(fs::read_to_string(endpoint).unwrap(), "replacement");
    }
}
