pub mod parse;
use crate::{
    daemon::{
        proto::{opt_str, req_str},
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    ssh::{shell_quote, RemoteExec},
};
use async_trait::async_trait;
use serde_json::{json, Value};
pub async fn read<E: RemoteExec>(exec: &E, path: &str) -> CoreResult<String> {
    if !path.starts_with('/') {
        return Err(CoreError::invalid("path", "expected absolute path"));
    }
    let out = exec
        .exec(&format!(
            "LC_ALL=C; export LC_ALL; head -c 1048577 -- {}",
            shell_quote(path)
        ))
        .await?;
    if !out.success() {
        return Err(super::command_failed("diff.read", &out));
    }
    if out.stdout.len() > 1048576 {
        return Err(CoreError::new("diff.too_large", "File exceeds one MiB"));
    }
    if out.stdout.contains(&0) {
        return Err(CoreError::new("diff.binary", "Binary files are not supported"));
    }
    String::from_utf8(out.stdout).map_err(|_| CoreError::new("remote.bad_output", "File is not UTF-8"))
}
pub struct DiffMethods;
#[async_trait]
impl MethodGroup for DiffMethods {
    fn prefix(&self) -> &'static str {
        "diff"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["diff.compare", "diff.save", "diff.versions.list", "diff.versions.del"]
    }
    async fn call(&self, cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
        if m == "diff.versions.list" {
            return cx
                .state
                .with_store(|s| super::notes::list(s, "diff", opt_str(&p, "connection_id")?.as_deref()));
        }
        if m == "diff.versions.del" {
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
            return cx
                .state
                .with_store(|s| super::notes::delete(s, "diff", &req_str(&p, "id")?));
        }
        let id = req_str(&p, "connection_id")?;
        let conn = cx.conn(&id).await?;
        let path = req_str(&p, "path")?;
        let left = read(conn.as_ref(), &path).await?;
        if m == "diff.save" {
            let v = json!({"id":crate::util::new_id(),"connection_id":id,"path":path,"content":left});
            cx.state
                .with_store(|s| super::notes::put(s, "diff", v["id"].as_str().unwrap_or(""), &v))?;
            return Ok(v);
        }
        let right = if let Some(version) = opt_str(&p, "version_id")? {
            cx.state.with_store(|s| super::notes::get(s, "diff", &version))?["content"]
                .as_str()
                .ok_or_else(|| CoreError::new("store.bad_document", "Invalid version"))?
                .to_owned()
        } else {
            let other = cx.conn(&req_str(&p, "other_connection_id")?).await?;
            read(other.as_ref(), &opt_str(&p, "other_path")?.unwrap_or(path)).await?
        };
        Ok(json!({"equal":left==right,"unified":parse::unified(&left,&right)}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn reads_and_checks() {
        let e = FakeExec::new().on("head", "hello\n", 0);
        assert_eq!(read(&e, "/etc/a'b").await.unwrap(), "hello\n");
        assert!(e.calls()[0].contains("LC_ALL=C"));
        assert!(e.calls()[0].contains("'\\''"));
        assert_eq!(
            read(&FakeExec::new(), "/a").await.unwrap_err().code,
            "remote.command_failed"
        );
        assert_eq!(
            read(&FakeExec::new().on("head", "a\0b", 0), "/a")
                .await
                .unwrap_err()
                .code,
            "diff.binary"
        );
    }
}
