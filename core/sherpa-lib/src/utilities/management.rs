//! Общая граница исполнения для системных утилит; парсеры остаются в группах.
use crate::error::{CoreError, CoreResult};
use crate::ssh::{ExecOutput, PooledConn, RemoteExec};

pub const LOCALE: &str = "LC_ALL=C; export LC_ALL; ";

pub fn bad(command: &str) -> CoreError {
    CoreError::new("remote.bad_output", "Unexpected remote output").with("command", command)
}

pub fn name(value: &str, field: &str) -> CoreResult<String> {
    if value.is_empty()
        || value.starts_with('-')
        || value.len() > 256
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.@:+-".contains(&b))
    {
        return Err(CoreError::invalid(
            field,
            "expected a name without options or shell syntax",
        ));
    }
    Ok(crate::ssh::shell_quote(value))
}

pub fn guarded(tool: &str, command: &str) -> String {
    format!("{LOCALE}command -v {tool} >/dev/null 2>&1 || {{ echo SHERPA_MISSING; exit 127; }}; {command}")
}

pub fn checked(label: &str, tool: &str, out: &ExecOutput) -> CoreResult<()> {
    if out.exit_status == Some(127) && out.stdout_lossy().trim() == "SHERPA_MISSING" {
        return Err(CoreError::new("cap.missing", "Required program is missing").with("capability", tool));
    }
    if !out.success() {
        return Err(super::command_failed(label, out));
    }
    Ok(())
}

/// Сборщики тестируются тем же RemoteExec; выбор sudo — только здесь.
pub struct Executor<'a> {
    pub conn: &'a PooledConn,
    pub privileged: bool,
}
impl RemoteExec for Executor<'_> {
    async fn exec_input(&self, command: &str, input: Option<&[u8]>) -> CoreResult<ExecOutput> {
        if self.privileged {
            self.conn.exec_privileged_io(command, input, None).await
        } else {
            self.conn.exec_input(command, input).await
        }
    }
    async fn exec_stream(
        &self,
        command: &str,
        input: Option<&[u8]>,
        output: tokio::sync::mpsc::Sender<(bool, Vec<u8>)>,
    ) -> CoreResult<ExecOutput> {
        if self.privileged {
            self.conn.exec_privileged_io(command, input, Some(output)).await
        } else {
            self.conn.exec_stream(command, input, output).await
        }
    }
}

/// Критичные действия требуют имени реального сервера, включённого в билет.
pub async fn confirm(
    cx: &crate::daemon::registry::Cx,
    conn: &PooledConn,
    method: &str,
    p: &serde_json::Value,
    critical: bool,
) -> CoreResult<()> {
    if critical {
        let expected = crate::daemon::proto::req_str(p, "confirm_hostname")?;
        let out = conn.exec("LC_ALL=C; export LC_ALL; hostname").await?;
        checked("hostname", "hostname", &out)?;
        if out.stdout_lossy().trim() != expected {
            return Err(CoreError::new(
                "danger.hostname_mismatch",
                "Server hostname confirmation does not match",
            ));
        }
    }
    cx.state.confirmations.consume(cx.client.id, method, p)
}
