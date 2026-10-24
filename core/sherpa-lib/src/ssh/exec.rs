//! Выполнение команд на сервере — основа утилит рабочего стола.
//! Утилита (monitor, disk, svc…) формирует команду, вызывает `exec` и парсит
//! вывод в JSON; фронт текст не разбирает (DESKTOP.md §5).

use russh::ChannelMsg;

use crate::error::{CoreError, CoreResult};
use crate::ssh::client::Session;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_status: Option<u32>,
}

impl ExecOutput {
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Абстракция над исполнителем: в тестах утилит её можно подменить.
#[allow(async_fn_in_trait)]
pub trait RemoteExec {
    async fn exec(&self, command: &str) -> CoreResult<ExecOutput>;
}

impl RemoteExec for Session {
    async fn exec(&self, command: &str) -> CoreResult<ExecOutput> {
        let fail = |e: russh::Error| CoreError::new("ssh.channel_failed", format!("Channel failed: {e}")).with("reason", e.to_string());
        let mut ch = self.handle.channel_open_session().await.map_err(fail)?;
        ch.exec(true, command).await.map_err(fail)?;
        let mut out = ExecOutput::default();
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => out.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(out)
    }
}
