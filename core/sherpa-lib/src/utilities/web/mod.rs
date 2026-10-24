use crate::daemon::proto::{opt_bool, opt_str, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};
pub mod parse;
pub fn command(method: &str, p: &Value) -> CoreResult<(String, bool)> {
    let engine = opt_str(p, "engine")?.unwrap_or_else(|| "nginx".into());
    let (tool, test, reload) = match engine.as_str() {
        "nginx" => ("nginx", "nginx -t", "nginx -s reload"),
        "apache" => ("apachectl", "apachectl configtest", "apachectl graceful"),
        _ => return Err(CoreError::invalid("engine", "nginx | apache")),
    };
    let mut write = false;
    let body=match method {
  "web.detect"=>return Ok((r#"LC_ALL=C; export LC_ALL; for t in nginx apachectl certbot openssl; do command -v "$t" >/dev/null 2>&1 && echo "$t"; done; true"#.into(),false)),
  "web.test"=>format!("{test} 2>&1"),
  "web.reload"=>{write=true;format!("{test} >/dev/null 2>&1 && {reload}")},
  "web.sites.list"=>if engine=="nginx" {"nginx -T 2>&1".into()}else{"apachectl -S 2>&1".into()},
  "web.sites.get"=>{
   let path=req_str(p,"path")?;
   let base=if engine=="nginx" {"/etc/nginx/"}else{"/etc/apache2/"};
   if !path.starts_with(base)||path.split('/').any(|v|v=="..")||path.contains('\n') {return Err(CoreError::invalid("path","configuration path required"));}
   format!("cat -- {}",shell_quote(&path))
  },
  "web.certs.list"=>return Ok((certificate_script(),false)),
  "web.certs.renew"=>{write=true;format!("certbot renew --non-interactive {}",if opt_bool(p,"dry_run")? {"--dry-run"}else{""})},
  _=>return Err(unknown(method))
 };
    if write && !opt_bool(p, "confirm")? {
        return Err(CoreError::new(
            "action.confirm_required",
            "Explicit confirmation required",
        ));
    }
    let required = if method == "web.certs.renew" { "certbot" } else { tool };
    Ok((
        format!("LC_ALL=C; export LC_ALL; command -v {required} >/dev/null 2>&1 || exit 127; {body}"),
        write,
    ))
}
pub fn decode(method: &str, p: &Value, out: &crate::ssh::ExecOutput) -> CoreResult<Value> {
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Web server tool is unavailable").with(
            "capability",
            if method.starts_with("web.certs") {
                "certbot"
            } else {
                "web"
            },
        ));
    }
    if method == "web.test" {
        return Ok(
            json!({"valid":out.success(),"lines":out.stdout_lossy().lines().chain(out.stderr_lossy().lines()).map(str::to_owned).collect::<Vec<_>>()}),
        );
    }
    if !out.success() {
        return Err(super::command_failed(method, out));
    }
    match method {
        "web.detect" => Ok(json!({"tools":out.stdout_lossy().lines().collect::<Vec<_>>()})),
        "web.sites.list" => parse::sites(&out.stdout_lossy(), opt_str(p, "engine")?.as_deref().unwrap_or("nginx")),
        "web.certs.list" => parse::certificate_sources(&out.stdout_lossy()),
        "web.sites.get" => Ok(json!({"path":req_str(p,"path")?,"content":out.stdout_lossy()})),
        _ => Ok(json!({"completed":true})),
    }
}
pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let (cmd, _) = command(method, p)?;
    decode(method, p, &exec.exec(&cmd).await?)
}
pub struct WebMethods;
#[async_trait]
impl MethodGroup for WebMethods {
    fn prefix(&self) -> &'static str {
        "web"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "web.detect",
            "web.sites.list",
            "web.sites.get",
            "web.test",
            "web.reload",
            "web.certs.list",
            "web.certs.renew",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let (cmd, write) = command(method, &p)?;
        let c = cx.conn(&req_str(&p, "connection_id")?).await?;
        if write {
            c.ensure_writable()?;
            cx.state.confirmations.consume(cx.client.id, method, &p)?;
        }
        decode(method, &p, &c.exec_privileged(&cmd).await?)
    }
}
fn certificate_script() -> String {
    r#"LC_ALL=C; export LC_ALL
command -v openssl >/dev/null 2>&1 || exit 127
if command -v certbot >/dev/null 2>&1; then certbot certificates || exit $?; else echo 'No certificates found.'; fi
echo '@@SHERPA_CERTS@@'
find /etc/nginx /etc/apache2 /etc/httpd -type f -exec awk '$1 == "ssl_certificate" || tolower($1) == "sslcertificatefile" {gsub(/[;\"]/ , "", $2); if ($2 ~ /^\//) print $2}' {} + 2>/dev/null | sort -u | while IFS= read -r path; do
  printf 'path=%s\n' "$path"
  openssl x509 -in "$path" -noout -enddate -subject || exit $?
  echo '@@SHERPA_CERT@@'
done
"#.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn test_failure_is_data() {
        let f = FakeExec::new().on("nginx -t", "nginx: invalid directive", 1);
        assert_eq!(collect(&f, "web.test", &json!({})).await.unwrap()["valid"], false);
    }
    #[test]
    fn reload_checks_before_apply() {
        assert!(command("web.reload", &json!({})).is_err());
        let (s, w) = command("web.reload", &json!({"confirm":true})).unwrap();
        assert!(w && s.contains("nginx -t >/dev/null 2>&1 && nginx -s reload"));
        assert!(command("web.sites.get", &json!({"path":"/etc/nginx/../shadow"})).is_err());
    }
    #[tokio::test]
    async fn missing() {
        assert_eq!(
            collect(&FakeExec::new(), "web.sites.list", &json!({}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
    }
}
