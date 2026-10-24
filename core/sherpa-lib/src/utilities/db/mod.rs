use crate::daemon::proto::{opt_bool, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};
pub mod parse;
pub fn command(method: &str, p: &Value) -> CoreResult<(String, bool)> {
    if method == "db.detect" {
        return Ok((r#"LC_ALL=C; export LC_ALL; for t in mysql mariadb psql redis-cli mysqldump mariadb-dump pg_dump; do command -v "$t" >/dev/null 2>&1 && echo "$t"; done; true"#.into(),false));
    }
    let engine = req_str(p, "engine")?;
    let tool = match engine.as_str() {
        "mysql" => "mysql",
        "mariadb" => "mariadb",
        "postgres" => "psql",
        "redis" => "redis-cli",
        _ => return Err(CoreError::invalid("engine", "mysql | mariadb | postgres | redis")),
    };
    let write = matches!(method, "db.kill" | "db.dump");
    if write && !opt_bool(p, "confirm")? {
        return Err(CoreError::new(
            "action.confirm_required",
            "Explicit confirmation required",
        ));
    }
    let query=match (method,engine.as_str()) {
 ("db.list","mysql"|"mariadb")=>"SELECT schema_name, COALESCE(SUM(data_length+index_length),0) FROM information_schema.schemata LEFT JOIN information_schema.tables USING(schema_name) GROUP BY schema_name".replace("USING(schema_name)","ON schema_name=table_schema"),
 ("db.queries","mysql"|"mariadb")=>"SELECT ID, USER, DB, TIME, STATE, INFO FROM information_schema.PROCESSLIST".into(),
 ("db.list","postgres")=>"SELECT datname, pg_database_size(oid), pg_get_userbyid(datdba) FROM pg_database ORDER BY datname".into(),
 ("db.queries","postgres")=>"SELECT pid, usename, datname, COALESCE(EXTRACT(EPOCH FROM now()-query_start)::bigint,0), state, query FROM pg_stat_activity WHERE pid <> pg_backend_pid()".into(),
 ("db.kill",_)=>{let id=opt_u64(p,"id")?.filter(|v|*v>0).ok_or_else(||CoreError::invalid("id","positive process id required"))?;match engine.as_str(){"mysql"|"mariadb"=>format!("KILL QUERY {id}"),"postgres"=>format!("SELECT pg_cancel_backend({id})"),"redis"=>format!("CLIENT KILL ID {id}"),_=>unreachable!()}},
 ("db.list","redis")=>"INFO keyspace".into(),
 ("db.queries","redis")=>"CLIENT LIST".into(),
 ("db.dump",_)=>String::new(),
 _=>return Err(unknown(method))
 };
    let body = if method == "db.dump" {
        let name = req_str(p, "database")?;
        if name.is_empty() || name.starts_with('-') || name.chars().any(|c| c == '\0' || c == '\n') {
            return Err(CoreError::invalid("database", "database name required"));
        }
        let path = req_str(p, "path")?;
        if !path.starts_with("/var/backups/") || path.split('/').any(|s| s == "..") || path.ends_with('/') {
            return Err(CoreError::invalid("path", "file under /var/backups required"));
        }
        let dump = match engine.as_str() {
            "mysql" => format!("mysqldump --single-transaction -- {}", shell_quote(&name)),
            "mariadb" => format!("mariadb-dump --single-transaction -- {}", shell_quote(&name)),
            "postgres" => format!(
                "pg_dump -Fc --dbname={}",
                shell_quote(&format!("dbname='{}'", name.replace('\\', "\\\\").replace('\'', "\\'")))
            ),
            "redis" => "redis-cli --rdb \"$tmp\"".into(),
            _ => unreachable!(),
        };
        // Build privately, then publish with an atomic hard link without overwriting.
        let output = if engine == "redis" {
            dump
        } else {
            format!("( {dump} ) > \"$tmp\"")
        };
        format!(
            "umask 077; tmp=$(mktemp /var/backups/.sherpa-dump.XXXXXXXX) || exit $?; trap 'rm -f -- \"$tmp\"' EXIT HUP INT TERM; {output} && ln -- \"$tmp\" {}",
            shell_quote(&path)
        )
    } else {
        match engine.as_str() {
            "mysql" | "mariadb" => format!("{tool} --batch --skip-column-names --execute {}", shell_quote(&query)),
            "postgres" => format!(
                "psql -X -w -A -t -F '\t' -v ON_ERROR_STOP=1 -d postgres -c {}",
                shell_quote(&query)
            ),
            _ => format!("redis-cli --raw {query}"),
        }
    };
    Ok((
        format!("LC_ALL=C; export LC_ALL; command -v {tool} >/dev/null 2>&1 || exit 127; {body}"),
        write,
    ))
}
fn decode(method: &str, p: &Value, out: &crate::ssh::ExecOutput) -> CoreResult<Value> {
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Database CLI unavailable")
            .with("capability", opt_str(p, "engine")?.unwrap_or_default()));
    }
    // CLI errors may contain credentials/configuration. Do not return stderr.
    if !out.success() {
        let mut safe = out.clone();
        safe.stderr.clear();
        return Err(super::command_failed(method, &safe));
    }
    if method == "db.detect" {
        return Ok(json!({"tools":out.stdout_lossy().lines().collect::<Vec<_>>()}));
    }
    if method == "db.dump" {
        return Ok(json!({"completed":true,"path":req_str(p,"path")?}));
    }
    if method == "db.kill" {
        if req_str(p, "engine")? == "postgres" && out.stdout_lossy().trim() != "t" {
            return Err(CoreError::new("db.cancel_failed", "Database query was not cancelled"));
        }
        if req_str(p, "engine")? == "redis" && out.stdout_lossy().trim() != "1" {
            return Err(CoreError::new("db.cancel_failed", "Redis client was not disconnected"));
        }
        return Ok(json!({"completed":true}));
    }
    parse::output(&out.stdout_lossy(), &req_str(p, "engine")?, method == "db.queries")
}
pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let (cmd, _) = command(method, p)?;
    decode(method, p, &exec.exec(&cmd).await?)
}
pub struct DbMethods;
#[async_trait]
impl MethodGroup for DbMethods {
    fn prefix(&self) -> &'static str {
        "db"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["db.detect", "db.list", "db.queries", "db.kill", "db.dump"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let (cmd, write) = command(method, &p)?;
        let c = cx.conn(&req_str(&p, "connection_id")?).await?;
        if write {
            c.ensure_writable()?;
        }
        decode(method, &p, &c.exec(&cmd).await?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn list() {
        let f = FakeExec::new().on("psql", "postgres\t8192\tpostgres\n", 0);
        let d = collect(&f, "db.list", &json!({"engine":"postgres"})).await.unwrap();
        assert_eq!(d["items"][0]["size_bytes"], 8192);
        assert!(f.calls()[0].contains("LC_ALL=C"));
    }
    #[test]
    fn guard() {
        assert!(command("db.kill", &json!({"engine":"mysql","id":3})).is_err());
        assert!(command(
            "db.dump",
            &json!({"engine":"mysql","confirm":true,"database":"test","path":"/var/backups/../../etc/passwd"})
        )
        .is_err());
    }
    #[tokio::test]
    async fn errors() {
        assert_eq!(
            collect(&FakeExec::new(), "db.list", &json!({"engine":"mysql"}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
        let f = FakeExec::new().on("mysql", "", 1);
        assert_eq!(
            collect(&f, "db.list", &json!({"engine":"mysql"}))
                .await
                .unwrap_err()
                .code,
            "remote.command_failed"
        );
    }
}
