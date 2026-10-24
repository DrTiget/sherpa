//! Выполнение команд на сервере — основа утилит рабочего стола.
//! Утилита (monitor, disk, svc…) формирует команду, вызывает `exec` и парсит
//! вывод в JSON; фронт текст не разбирает (DESKTOP.md §5).

use russh::client::Handle;
use russh::ChannelMsg;

use crate::error::{CoreError, CoreResult};
use crate::ssh::client::{Session, Verifier};

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
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    pub fn success(&self) -> bool {
        self.exit_status == Some(0)
    }
}

/// Исполнитель команд. В тестах утилит подменяется (`FakeExec`).
#[allow(async_fn_in_trait)]
pub trait RemoteExec {
    /// Выполнить команду (через `sh` пользователя), передав `input` в stdin.
    async fn exec_input(&self, command: &str, input: Option<&[u8]>) -> CoreResult<ExecOutput>;

    /// Поток stdout/stderr с ограниченной очередью. В тестах — один блок на поток.
    async fn exec_stream(
        &self,
        command: &str,
        input: Option<&[u8]>,
        output: tokio::sync::mpsc::Sender<(bool, Vec<u8>)>,
    ) -> CoreResult<ExecOutput> {
        let out = self.exec_input(command, input).await?;
        for (stderr, data) in [(false, &out.stdout), (true, &out.stderr)] {
            if !data.is_empty() {
                let _ = output.send((stderr, data.clone())).await;
            }
        }
        Ok(out)
    }

    async fn exec(&self, command: &str) -> CoreResult<ExecOutput> {
        self.exec_input(command, None).await
    }
}

fn channel_error(e: russh::Error) -> CoreError {
    CoreError::new("ssh.channel_failed", format!("Channel failed: {e}")).with("reason", e.to_string())
}

/// Выполнить команду в новом канале `exec` поверх готового соединения.
pub async fn exec_on(handle: &Handle<Verifier>, command: &str, input: Option<&[u8]>) -> CoreResult<ExecOutput> {
    exec_on_stream(handle, command, input, None).await
}

pub async fn exec_on_stream(
    handle: &Handle<Verifier>,
    command: &str,
    input: Option<&[u8]>,
    output: Option<tokio::sync::mpsc::Sender<(bool, Vec<u8>)>>,
) -> CoreResult<ExecOutput> {
    let mut ch = handle.channel_open_session().await.map_err(channel_error)?;
    ch.exec(true, command).await.map_err(channel_error)?;
    if let Some(data) = input {
        ch.data(data).await.map_err(channel_error)?;
    }
    ch.eof().await.map_err(channel_error)?;
    let mut out = ExecOutput::default();
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                if let Some(tx) = &output {
                    let _ = tx.send((false, data.to_vec())).await;
                }
                if output.is_none() || out.stdout.len() < 4096 {
                    out.stdout.extend_from_slice(&data);
                }
            }
            ChannelMsg::ExtendedData { data, .. } => {
                if let Some(tx) = &output {
                    let _ = tx.send((true, data.to_vec())).await;
                }
                // Потоковые операции не накапливают неограниченный вывод.
                if output.is_none() || out.stderr.len() < 4096 {
                    out.stderr.extend_from_slice(&data);
                }
            }
            ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    Ok(out)
}

impl RemoteExec for Session {
    async fn exec_input(&self, command: &str, input: Option<&[u8]>) -> CoreResult<ExecOutput> {
        exec_on(&self.handle, command, input).await
    }
}

/// Экранирование для POSIX sh: `'…'` с заменой `'` на `'\''`.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Подменный исполнитель для тестов утилит: ответ по подстроке команды.
#[derive(Default)]
pub struct FakeExec {
    pub rules: Vec<(String, ExecOutput)>,
    pub calls: std::sync::Mutex<Vec<String>>,
}

impl FakeExec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Если команда содержит `needle` — вернуть `stdout` с кодом `code`.
    pub fn on(mut self, needle: &str, stdout: &str, code: u32) -> Self {
        self.rules.push((
            needle.to_string(),
            ExecOutput {
                stdout: stdout.as_bytes().to_vec(),
                stderr: vec![],
                exit_status: Some(code),
            },
        ));
        self
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }
}

impl RemoteExec for FakeExec {
    async fn exec_input(&self, command: &str, _input: Option<&[u8]>) -> CoreResult<ExecOutput> {
        self.calls.lock().expect("calls").push(command.to_string());
        for (needle, out) in &self.rules {
            if command.contains(needle.as_str()) {
                return Ok(out.clone());
            }
        }
        Ok(ExecOutput {
            stdout: vec![],
            stderr: b"command not found".to_vec(),
            exit_status: Some(127),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
