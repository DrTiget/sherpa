//! Очередь передач; 2 активные задачи, ограниченные буферы, проверка SHA-256.
use super::{
    files::{child, io_error, path, sftp_error},
    jobs::Job,
};
use crate::{
    daemon::{
        proto::{opt_bool, req_str},
        registry::{Cx, MethodGroup},
        state::DaemonState,
    },
    error::{CoreError, CoreResult},
    ssh::PooledConn,
};
use async_trait::async_trait;
use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

pub struct TransferMethods;
#[async_trait]
impl MethodGroup for TransferMethods {
    fn prefix(&self) -> &'static str {
        "transfer"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["transfer.start", "transfer.list", "transfer.cancel"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if method == "transfer.list" {
            return Ok(cx.state.jobs.list("transfer"));
        }
        if method == "transfer.cancel" {
            let j = cx.state.jobs.get(&req_str(&p, "id")?, "transfer")?;
            j.cancel.send_replace(true);
            return Ok(json!({"cancelled":true}));
        }
        let direction = req_str(&p, "direction")?;
        if !matches!(direction.as_str(), "upload" | "download") {
            return Err(CoreError::invalid("direction", "expected upload or download"));
        }
        let remote = path(&p, "remote")?;
        let local = PathBuf::from(req_str(&p, "local")?);
        if !local.is_absolute() || local.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(CoreError::invalid(
                "local",
                "expected absolute local path without parent components",
            ));
        }
        let resume = opt_bool(&p, "resume")?;
        let connection = req_str(&p, "connection_id")?;
        let conn = cx.conn(&connection).await?;
        if direction == "upload" {
            conn.ensure_writable()?;
        }
        let j = cx.state.jobs.insert(
            cx.state.new_id("x"),
            "transfer",
            connection,
            json!({"status":"queued","direction":direction,"local":local,"remote":remote}),
        )?;
        let id = j.id.clone();
        let state = cx.state.clone();
        let mut cancel = j.cancel.subscribe();
        tokio::spawn(async move {
            let work = async {
                let _permit = state
                    .jobs
                    .transfers
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| CoreError::internal("transfer semaphore closed"))?;
                j.update(&state, json!({"status":"running","bytes":0})).await;
                let s = conn.sftp().await?;
                run(&s, &conn, &state, &j, &local, &remote, direction == "upload", resume).await
            };
            let result = tokio::select! { biased; _ = cancel.wait_for(|v| *v) => Err(CoreError::new("transfer.cancelled","Transfer cancelled")), r = work => r };
            match result {
                Ok(bytes) => {
                    j.update(&state, json!({"status":"completed","bytes":bytes,"verified":"sha256"}))
                        .await
                }
                Err(e) => {
                    j.update(
                        &state,
                        json!({"status":if e.code == "transfer.cancelled" {"cancelled"} else {"failed"},"error":e}),
                    )
                    .await
                }
            }
        });
        Ok(json!({"id":id,"status":"queued"}))
    }
}
async fn safe_local(path: &Path) -> CoreResult<()> {
    let mut parent = Some(path);
    while let Some(p) = parent {
        match tokio::fs::symlink_metadata(p).await {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(CoreError::new("transfer.symlink", "Local symlinks are not followed"))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(e)),
        }
        parent = p.parent();
    }
    Ok(())
}
async fn remote_meta(s: &SftpSession, path: &str) -> CoreResult<Option<russh_sftp::protocol::FileAttributes>> {
    match s.symlink_metadata(path).await {
        Ok(a) if a.is_symlink() => Err(CoreError::new("transfer.symlink", "Remote symlinks are not followed")),
        Ok(a) => Ok(Some(a)),
        Err(e) => {
            let e = sftp_error(e);
            if e.code == "fs.not_found" {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
async fn run(
    s: &SftpSession,
    conn: &PooledConn,
    state: &DaemonState,
    job: &Job,
    local: &Path,
    remote: &str,
    upload: bool,
    resume: bool,
) -> CoreResult<u64> {
    let mut stack = vec![(local.to_path_buf(), remote.to_string())];
    let mut total = 0;
    let mut count = 0;
    while let Some((local, remote)) = stack.pop() {
        count += 1;
        if count > 100_000 {
            return Err(CoreError::new("fs.limit", "Traversal limit exceeded"));
        }
        safe_local(&local).await?;
        // Проверяем и родительские компоненты удалённого пути.
        let mut prefix = String::new();
        for c in remote.split('/').filter(|c| !c.is_empty()) {
            prefix.push('/');
            prefix.push_str(c);
            remote_meta(s, &prefix).await?;
        }
        if upload {
            conn.ensure_writable()?;
        }
        let rm = remote_meta(s, &remote).await?;
        let is_dir = if upload {
            tokio::fs::symlink_metadata(&local).await.map_err(io_error)?.is_dir()
        } else {
            rm.as_ref()
                .ok_or_else(|| CoreError::new("fs.not_found", "Remote source not found"))?
                .is_dir()
        };
        if is_dir {
            if upload {
                if rm.is_none() {
                    s.create_dir(&remote).await.map_err(sftp_error)?;
                }
                let mut dir = tokio::fs::read_dir(&local).await.map_err(io_error)?;
                while let Some(e) = dir.next_entry().await.map_err(io_error)? {
                    let name = e
                        .file_name()
                        .into_string()
                        .map_err(|_| CoreError::new("fs.encoding", "Expected UTF-8 filename"))?;
                    stack.push((e.path(), child(&remote, &name)?));
                }
            } else {
                tokio::fs::create_dir_all(&local).await.map_err(io_error)?;
                for e in s.read_dir(&remote).await.map_err(sftp_error)? {
                    let name = e.file_name();
                    let r = child(&remote, &name)?;
                    stack.push((local.join(name), r));
                }
            }
        } else {
            let bytes = if upload {
                upload_file(s, conn, state, job, &local, &remote, resume, total).await?
            } else {
                download_file(s, state, job, &local, &remote, resume, total).await?
            };
            total += bytes;
        }
    }
    Ok(total)
}
async fn hash<R: AsyncRead + Unpin>(mut reader: R) -> CoreResult<(u64, Vec<u8>)> {
    let mut h = Sha256::new();
    let mut bytes = 0;
    let mut buf = [0; 65536];
    loop {
        let n = reader.read(&mut buf).await.map_err(io_error)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        bytes += n as u64;
    }
    Ok((bytes, h.finalize().to_vec()))
}
async fn prefix_matches<R: AsyncRead + Unpin, S: AsyncRead + Unpin>(a: &mut R, b: &mut S, size: u64) -> CoreResult<()> {
    if hash(a.take(size)).await? != hash(b.take(size)).await? {
        return Err(CoreError::new("transfer.conflict", "Resume prefix differs from source"));
    }
    Ok(())
}
async fn copy<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut src: R,
    mut dst: W,
    state: &DaemonState,
    job: &Job,
    offset: u64,
    conn: Option<&PooledConn>,
) -> CoreResult<u64> {
    let mut buf = [0; 65536];
    let mut bytes = offset;
    let mut last = std::time::Instant::now();
    loop {
        if let Some(c) = conn {
            c.ensure_writable()?;
        }
        let n = src.read(&mut buf).await.map_err(io_error)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await.map_err(io_error)?;
        bytes += n as u64;
        if last.elapsed() >= std::time::Duration::from_millis(200) {
            job.update(state, json!({"status":"running","bytes":bytes})).await;
            last = std::time::Instant::now();
        }
    }
    dst.flush().await.map_err(io_error)?;
    Ok(bytes - offset)
}
#[allow(clippy::too_many_arguments)]
async fn upload_file(
    s: &SftpSession,
    conn: &PooledConn,
    state: &DaemonState,
    job: &Job,
    local: &Path,
    remote: &str,
    resume: bool,
    total: u64,
) -> CoreResult<u64> {
    let mut src = tokio::fs::File::open(local).await.map_err(io_error)?;
    let size = src.metadata().await.map_err(io_error)?.len();
    if !src.metadata().await.map_err(io_error)?.is_file() {
        return Err(CoreError::new("fs.not_file", "Expected regular file"));
    }
    let existing = remote_meta(s, remote).await?;
    if existing.as_ref().is_some_and(|a| !a.is_regular()) {
        return Err(CoreError::new("fs.not_file", "Expected regular file"));
    }
    if existing.is_some() && !resume {
        return Err(CoreError::new(
            "fs.exists",
            "Destination exists; resume must be explicit",
        ));
    }
    let offset = existing.as_ref().and_then(|a| a.size).unwrap_or(0);
    if offset > size {
        return Err(CoreError::new("transfer.conflict", "Destination is larger than source"));
    }
    if offset > 0 {
        let mut dst = s.open(remote).await.map_err(sftp_error)?;
        prefix_matches(&mut src, &mut dst, offset).await?;
        dst.close().await.map_err(io_error)?;
    }
    src.seek(std::io::SeekFrom::Start(offset)).await.map_err(io_error)?;
    conn.ensure_writable()?;
    let flags = if existing.is_some() {
        OpenFlags::WRITE
    } else {
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE
    };
    let mut dst = s.open_with_flags(remote, flags).await.map_err(sftp_error)?;
    dst.seek(std::io::SeekFrom::Start(offset)).await.map_err(io_error)?;
    copy(&mut src, &mut dst, state, job, total + offset, Some(conn)).await?;
    dst.close().await.map_err(io_error)?;
    src.rewind().await.map_err(io_error)?;
    let mut remote_file = s.open(remote).await.map_err(sftp_error)?;
    let check = hash(&mut remote_file).await?;
    remote_file.close().await.map_err(io_error)?;
    if hash(src).await? != check || check.0 != size {
        return Err(CoreError::new(
            "transfer.integrity",
            "Size or SHA-256 verification failed",
        ));
    }
    Ok(size)
}
async fn download_file(
    s: &SftpSession,
    state: &DaemonState,
    job: &Job,
    local: &Path,
    remote: &str,
    resume: bool,
    total: u64,
) -> CoreResult<u64> {
    let mut src = s.open(remote).await.map_err(sftp_error)?;
    let a = src.metadata().await.map_err(sftp_error)?;
    if !a.is_regular() {
        return Err(CoreError::new("fs.not_file", "Expected regular file"));
    }
    let size = a.size.ok_or_else(|| CoreError::new("fs.sftp", "Missing file size"))?;
    let existing = match tokio::fs::metadata(local).await {
        Ok(m) => Some(m),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(e)),
    };
    if existing.as_ref().is_some_and(|m| !m.is_file()) {
        return Err(CoreError::new("fs.not_file", "Expected regular file"));
    }
    if existing.is_some() && !resume {
        return Err(CoreError::new(
            "fs.exists",
            "Destination exists; resume must be explicit",
        ));
    }
    let offset = existing.as_ref().map(|m| m.len()).unwrap_or(0);
    if offset > size {
        return Err(CoreError::new("transfer.conflict", "Destination is larger than source"));
    }
    if offset > 0 {
        let mut dst = tokio::fs::File::open(local).await.map_err(io_error)?;
        prefix_matches(&mut src, &mut dst, offset).await?;
    }
    src.seek(std::io::SeekFrom::Start(offset)).await.map_err(io_error)?;
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true);
    if existing.is_none() {
        opts.create_new(true);
    }
    let mut dst = opts.open(local).await.map_err(io_error)?;
    dst.seek(std::io::SeekFrom::Start(offset)).await.map_err(io_error)?;
    copy(&mut src, &mut dst, state, job, total + offset, None).await?;
    dst.sync_all().await.map_err(io_error)?;
    drop(dst);
    src.rewind().await.map_err(io_error)?;
    let check = hash(&mut src).await?;
    src.close().await.map_err(io_error)?;
    if hash(tokio::fs::File::open(local).await.map_err(io_error)?).await? != check || check.0 != size {
        return Err(CoreError::new(
            "transfer.integrity",
            "Size or SHA-256 verification failed",
        ));
    }
    Ok(size)
}
