//! Локальный транспорт демона: Unix socket (Linux) или named pipe (Windows).
//! Доступ — только у текущего пользователя ОС.

use std::path::Path;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::{CoreError, CoreResult};

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

pub type BoxStream = Box<dyn Stream>;

fn bind_error(endpoint: &Path, e: &std::io::Error) -> CoreError {
    CoreError::new(
        "daemon.start_failed",
        format!("Cannot listen on {}: {e}", endpoint.display()),
    )
    .with("reason", e.to_string())
}

#[cfg(unix)]
pub struct Listener {
    inner: tokio::net::UnixListener,
}

#[cfg(unix)]
impl Listener {
    /// Слушать сокет: каталог 700, сокет 600. Старый файл сокета удаляется —
    /// вызывающий уже держит lock-файл, значит живого демона нет.
    pub fn bind(endpoint: &Path) -> CoreResult<Listener> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = endpoint.parent() {
            crate::store::crypto::create_private_dir(dir)?;
        }
        let _ = std::fs::remove_file(endpoint);
        let inner = tokio::net::UnixListener::bind(endpoint).map_err(|e| bind_error(endpoint, &e))?;
        std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| bind_error(endpoint, &e))?;
        Ok(Listener { inner })
    }

    pub async fn accept(&mut self) -> std::io::Result<BoxStream> {
        let (s, _) = self.inner.accept().await?;
        Ok(Box::new(s))
    }
}

#[cfg(unix)]
pub async fn connect(endpoint: &Path) -> std::io::Result<BoxStream> {
    Ok(Box::new(tokio::net::UnixStream::connect(endpoint).await?))
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

    /// DACL: полный доступ только владельцу (создателю канала) и SYSTEM.
    const SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

    pub fn create(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
        let wide: Vec<u16> = SDDL.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sd: *mut c_void = std::ptr::null_mut();
        // SAFETY: строка SDDL завершена нулём; sd освобождается через LocalFree.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let mut opts = ServerOptions::new();
        opts.first_pipe_instance(first).reject_remote_clients(true);
        // SAFETY: sa и sd живы до конца вызова.
        let r = unsafe { opts.create_with_security_attributes_raw(name, &mut sa as *mut _ as *mut c_void) };
        // SAFETY: sd выделен ConvertString…, после создания канала не нужен.
        unsafe { LocalFree(sd) };
        r
    }
}

#[cfg(windows)]
pub struct Listener {
    name: String,
    next: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl Listener {
    pub fn bind(endpoint: &Path) -> CoreResult<Listener> {
        let name = endpoint.display().to_string();
        let next = win::create(&name, true).map_err(|e| bind_error(endpoint, &e))?;
        Ok(Listener { name, next })
    }

    pub async fn accept(&mut self) -> std::io::Result<BoxStream> {
        self.next.connect().await?;
        let fresh = win::create(&self.name, false)?;
        let connected = std::mem::replace(&mut self.next, fresh);
        Ok(Box::new(connected))
    }
}

#[cfg(windows)]
pub async fn connect(endpoint: &Path) -> std::io::Result<BoxStream> {
    use tokio::net::windows::named_pipe::ClientOptions;
    const ERROR_PIPE_BUSY: i32 = 231;
    let name = endpoint.display().to_string();
    for _ in 0..40 {
        match ClientOptions::new().open(&name) {
            Ok(c) => return Ok(Box::new(c)),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "named pipe busy"))
}
