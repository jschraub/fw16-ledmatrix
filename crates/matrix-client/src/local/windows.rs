use super::Stream;
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

fn process_sid(process: HANDLE) -> io::Result<String> {
    // SAFETY: handles and output buffers are kept alive for each Windows call;
    // token data uses pointer-aligned storage, and allocated SID strings are freed.
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut bytes = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut bytes,
        );
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            storage.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = &*storage.as_ptr().cast::<TOKEN_USER>();
        let mut text = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(sid)
    }
}

fn current_sid() -> io::Result<String> {
    // SAFETY: this is a borrowed pseudo handle, never closed by process_sid.
    process_sid(unsafe { GetCurrentProcess() })
}

pub fn default_endpoint() -> io::Result<String> {
    // SID avoids collisions between domain/local users with the same username.
    Ok(format!(r"\\.\pipe\fw16-matrix-{}", current_sid()?))
}

fn validate_endpoint(endpoint: &str) -> io::Result<()> {
    if !endpoint.strip_prefix(r"\\.\pipe\").is_some_and(|name| {
        !name.is_empty() && name.len() <= 200 && !name.contains(['\\', '/', '\0'])
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only local named-pipe endpoints are supported",
        ));
    }
    Ok(())
}

pub async fn connect(endpoint: &str) -> io::Result<Stream> {
    validate_endpoint(endpoint)?;
    for _ in 0..100 {
        match ClientOptions::new().open(endpoint) {
            Ok(stream) => {
                // Authenticate the server as well: do not trust pipe name ownership.
                unsafe {
                    let mut pid = 0;
                    if GetNamedPipeServerProcessId(stream.as_raw_handle(), &mut pid) == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if process.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let process = OwnedHandle::from_raw_handle(process);
                    if process_sid(process.as_raw_handle())? != current_sid()? {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "daemon belongs to a different user",
                        ));
                    }
                }
                return Ok(Box::new(stream));
            }
            Err(e) if e.raw_os_error() == Some(231) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "daemon pipe busy"))
}

fn create(endpoint: &str, first: bool) -> io::Result<NamedPipeServer> {
    validate_endpoint(endpoint)?;
    // Protected DACL, one allow ACE: only the current user's SID, not Everyone,
    // administrators as a group, or the inherited default DACL.
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})\0", current_sid()?)
        .encode_utf16()
        .collect();
    unsafe {
        let mut descriptor = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        // Kernel copies the descriptor during creation, before LocalFree.
        let result = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                endpoint,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            );
        LocalFree(descriptor);
        result
    }
}

pub struct Listener {
    inner: Option<NamedPipeServer>,
    endpoint: String,
}

impl Listener {
    pub async fn bind(endpoint: &str) -> io::Result<Self> {
        Ok(Self {
            inner: Some(create(endpoint, true)?),
            endpoint: endpoint.into(),
        })
    }

    pub async fn accept(&mut self) -> io::Result<Stream> {
        self.inner.as_ref().expect("pending pipe").connect().await?;
        // Always keep one pending instance so ownership never lapses between clients.
        let next = create(&self.endpoint, false)?;
        Ok(Box::new(self.inner.replace(next).expect("connected pipe")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::{
        Authorization::ConvertStringSidToSidW, CreateRestrictedToken, DuplicateTokenEx,
        RevertToSelf, SID_AND_ATTRIBUTES, SecurityImpersonation, TOKEN_DUPLICATE,
        TOKEN_IMPERSONATE, TokenImpersonation,
    };
    use windows_sys::Win32::System::Threading::SetThreadToken;

    #[test]
    fn rejects_network_pipe_paths() {
        assert!(validate_endpoint(r"\\remote-host\pipe\matrix").is_err());
        assert!(validate_endpoint(r"\\.\pipe\matrix").is_ok());
    }

    #[tokio::test]
    async fn protected_pipe_denies_a_restricted_client_but_accepts_its_owner() {
        let directory = tempfile::tempdir().unwrap();
        let endpoint = format!(
            r"\\.\pipe\matrix-acl-{}-{}",
            std::process::id(),
            directory.path().file_name().unwrap().to_string_lossy()
        );
        let _listener = Listener::bind(&endpoint).await.unwrap();
        // Restrict the current token to World for the second ACL check. The pipe's
        // one-user DACL must deny it. This needs no second account or elevation.
        // No await or assertion occurs while the test thread is impersonating.
        unsafe {
            let mut primary = ptr::null_mut();
            assert_ne!(
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut primary
                ),
                0
            );
            let primary = OwnedHandle::from_raw_handle(primary);
            let mut impersonation = ptr::null_mut();
            assert_ne!(
                DuplicateTokenEx(
                    primary.as_raw_handle(),
                    TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE,
                    ptr::null(),
                    SecurityImpersonation,
                    TokenImpersonation,
                    &mut impersonation
                ),
                0
            );
            let impersonation = OwnedHandle::from_raw_handle(impersonation);
            let mut world = ptr::null_mut();
            let text: Vec<u16> = "S-1-1-0\0".encode_utf16().collect();
            assert_ne!(ConvertStringSidToSidW(text.as_ptr(), &mut world), 0);
            let sid = SID_AND_ATTRIBUTES {
                Sid: world,
                Attributes: 0,
            };
            let mut restricted = ptr::null_mut();
            let created = CreateRestrictedToken(
                impersonation.as_raw_handle(),
                0,
                0,
                ptr::null(),
                0,
                ptr::null(),
                1,
                &sid,
                &mut restricted,
            );
            LocalFree(world);
            assert_ne!(created, 0);
            let restricted = OwnedHandle::from_raw_handle(restricted);
            assert_ne!(SetThreadToken(ptr::null(), restricted.as_raw_handle()), 0);
            let denied = ClientOptions::new().open(&endpoint);
            let reverted = RevertToSelf();
            assert_ne!(reverted, 0);
            assert_eq!(
                denied.expect_err("restricted token must be denied").kind(),
                io::ErrorKind::PermissionDenied
            );
        }
        assert!(connect(&endpoint).await.is_ok());
    }
}
