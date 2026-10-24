//! Файловые операции через отдельный SFTP-канал общего пула.
use crate::{
    daemon::{
        proto::{opt_bool, opt_u64, req_str},
        registry::{unknown, Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    ssh::shell_quote,
};
use async_trait::async_trait;
use russh_sftp::{
    client::{error::Error, SftpSession},
    protocol::{FileAttributes, StatusCode},
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub const TEXT_LIMIT: usize = 128 * 1024;
pub fn sftp_error(e: Error) -> CoreError {
    let code = match e {
        Error::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => "fs.not_found",
            StatusCode::PermissionDenied => "fs.permission",
            _ => "fs.sftp",
        },
        Error::Timeout => "fs.timeout",
        _ => "fs.sftp",
    };
    CoreError::new(code, "SFTP operation failed")
}
pub fn io_error(_: std::io::Error) -> CoreError {
    CoreError::new("fs.io", "File I/O failed")
}
pub fn path(p: &Value, key: &str) -> CoreResult<String> {
    let s = req_str(p, key)?;
    if s.contains('\0') || !s.starts_with('/') || s.split('/').any(|c| c == "..") {
        return Err(CoreError::invalid(
            key,
            "expected absolute POSIX path without parent components",
        ));
    }
    Ok(s)
}
pub fn child(parent: &str, name: &str) -> CoreResult<String> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0', ':']) {
        return Err(CoreError::new("fs.bad_name", "Unsafe directory entry"));
    }
    Ok(format!("{}/{name}", parent.trim_end_matches('/')))
}
fn entry(path: &str, a: &FileAttributes) -> Value {
    json!({"path":path, "name":path.rsplit('/').next().unwrap_or(""), "size":a.size, "uid":a.uid, "gid":a.gid, "permissions":a.permissions, "mtime":a.mtime,
        "type":if a.is_dir() {"directory"} else if a.is_symlink() {"symlink"} else if a.is_regular() {"file"} else {"other"}})
}
pub async fn delete_tree(s: &SftpSession, root: &str, recursive: bool) -> CoreResult<()> {
    let mut stack = vec![(root.to_string(), false)];
    let mut visited = 0;
    while let Some((p, expanded)) = stack.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err(CoreError::new("fs.limit", "Traversal limit exceeded"));
        }
        let a = s.symlink_metadata(&p).await.map_err(sftp_error)?;
        if a.is_dir() {
            if recursive && !expanded {
                stack.push((p.clone(), true));
                for e in s.read_dir(&p).await.map_err(sftp_error)? {
                    stack.push((child(&p, &e.file_name())?, false));
                }
            } else {
                s.remove_dir(p).await.map_err(sftp_error)?;
            }
        } else {
            s.remove_file(p).await.map_err(sftp_error)?;
        }
    }
    Ok(())
}
pub async fn read_text(s: &SftpSession, p: &str) -> CoreResult<Value> {
    let mut f = s.open(p).await.map_err(sftp_error)?;
    let a = f.metadata().await.map_err(sftp_error)?;
    if !a.is_regular() {
        return Err(CoreError::new("fs.not_file", "Expected regular file"));
    }
    let mut data = Vec::new();
    (&mut f)
        .take((TEXT_LIMIT + 1) as u64)
        .read_to_end(&mut data)
        .await
        .map_err(io_error)?;
    f.close().await.map_err(io_error)?;
    if data.len() > TEXT_LIMIT {
        return Err(CoreError::new("fs.too_large", "Text file exceeds size limit"));
    }
    let text = String::from_utf8(data).map_err(|_| CoreError::new("fs.encoding", "Expected UTF-8 text"))?;
    if text.contains('\0') {
        return Err(CoreError::new("fs.encoding", "Binary file is not editable"));
    }
    Ok(json!({"text":text,"mtime":a.mtime,"size":a.size}))
}
pub struct FilesMethods;
#[async_trait]
impl MethodGroup for FilesMethods {
    fn prefix(&self) -> &'static str {
        "files"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "files.list",
            "files.stat",
            "files.mkdir",
            "files.rename",
            "files.delete",
            "files.chmod",
            "files.chown",
            "files.read",
            "files.write",
            "files.search",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        let target = path(&p, "path")?;
        let modifying = !matches!(method, "files.list" | "files.stat" | "files.read" | "files.search");
        if modifying {
            conn.ensure_writable()?;
        }
        if method == "files.delete" {
            if target.trim_matches('/').is_empty() {
                return Err(CoreError::invalid("path", "cannot delete root"));
            }
            if opt_bool(&p, "recursive")? {
                cx.state.confirmations.consume(cx.client.id, method, &p)?;
            }
        }
        // SFTP не повышает права: явно запрошенный root идёт через priv.*.
        if matches!(method, "files.chmod" | "files.chown") && opt_bool(&p, "privileged")? {
            let command = if method == "files.chmod" {
                format!("chmod {:o} -- {}", number(&p, "mode", 0o7777)?, shell_quote(&target))
            } else {
                format!(
                    "chown {}:{} -- {}",
                    number(&p, "uid", u32::MAX)?,
                    number(&p, "gid", u32::MAX)?,
                    shell_quote(&target)
                )
            };
            let out = conn.exec_privileged(&command).await?;
            if !out.success() {
                return Err(CoreError::new("fs.permission", "Privileged metadata update failed"));
            }
            return Ok(Value::Null);
        }
        let s = conn.sftp().await?;
        match method {
            "files.stat" => Ok(entry(
                &target,
                &s.symlink_metadata(target.clone()).await.map_err(sftp_error)?,
            )),
            "files.list" | "files.search" => {
                let query = if method == "files.search" {
                    req_str(&p, "query")?
                } else {
                    String::new()
                };
                let limit = opt_u64(&p, "limit")?.unwrap_or(1000).clamp(1, 5000) as usize;
                let mut dirs = vec![target];
                let mut items = Vec::new();
                let mut visited = 0;
                while let Some(dir) = dirs.pop() {
                    for e in s.read_dir(&dir).await.map_err(sftp_error)? {
                        visited += 1;
                        let full = child(&dir, &e.file_name())?;
                        if e.file_name().contains(&query) {
                            items.push(entry(&full, &e.metadata()));
                        }
                        if items.len() >= limit || visited >= 100_000 {
                            return Ok(json!({"items":items,"truncated":true}));
                        }
                        if method == "files.search" && e.file_type().is_dir() {
                            dirs.push(full);
                        }
                    }
                }
                Ok(json!({"items":items,"truncated":false}))
            }
            "files.read" => read_text(&s, &target).await,
            "files.write" => {
                let text = p
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| CoreError::required("text"))?;
                if text.len() > TEXT_LIMIT {
                    return Err(CoreError::new("fs.too_large", "Text file exceeds size limit"));
                }
                if text.contains('\0') {
                    return Err(CoreError::new("fs.encoding", "Binary file is not editable"));
                }
                let expected = number(&p, "mtime", u32::MAX)?;
                let a = s.symlink_metadata(&target).await.map_err(sftp_error)?;
                if !a.is_regular() {
                    return Err(CoreError::new("fs.not_file", "Expected regular file"));
                }
                if a.mtime != Some(expected) {
                    return Err(
                        CoreError::new("fs.conflict", "File was modified since it was read").with("mtime", a.mtime)
                    );
                }
                let mut f = s.create(&target).await.map_err(sftp_error)?;
                f.write_all(text.as_bytes()).await.map_err(io_error)?;
                f.close().await.map_err(io_error)?;
                Ok(entry(&target, &s.metadata(target.clone()).await.map_err(sftp_error)?))
            }
            "files.mkdir" => {
                s.create_dir(target).await.map_err(sftp_error)?;
                Ok(Value::Null)
            }
            "files.rename" => {
                s.rename(target, path(&p, "to")?).await.map_err(sftp_error)?;
                Ok(Value::Null)
            }
            "files.delete" => {
                delete_tree(&s, &target, opt_bool(&p, "recursive")?).await?;
                Ok(Value::Null)
            }
            "files.chmod" | "files.chown" => {
                let mut a = FileAttributes::default();
                if method == "files.chmod" {
                    a.permissions = Some(number(&p, "mode", 0o7777)?);
                } else {
                    a.uid = Some(number(&p, "uid", u32::MAX)?);
                    a.gid = Some(number(&p, "gid", u32::MAX)?);
                }
                s.set_metadata(target, a).await.map_err(sftp_error)?;
                Ok(Value::Null)
            }
            _ => Err(unknown(method)),
        }
    }
}
pub fn number(p: &Value, key: &str, max: u32) -> CoreResult<u32> {
    opt_u64(p, key)?
        .filter(|n| *n <= max as u64)
        .map(|n| n as u32)
        .ok_or_else(|| CoreError::invalid(key, "missing or out of range"))
}
