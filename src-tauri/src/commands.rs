use crate::AppState;
use servergui_core::{
    model::{ExecResult, Server, ServerInput},
    CoreError,
};
use servergui_core::terminal::TermEvent;
use tauri::{ipc::Channel, State};

type R<T> = Result<T, CoreError>;

#[tauri::command]
pub fn list_servers(s: State<AppState>) -> R<Vec<Server>> {
    s.store.list()
}

#[tauri::command]
pub fn create_server(s: State<AppState>, input: ServerInput) -> R<Server> {
    s.store.create(input)
}

#[tauri::command]
pub async fn update_server(s: State<'_, AppState>, id: String, input: ServerInput) -> R<Server> {
    let srv = s.store.update(&id, input)?;
    // параметры могли поменяться — старая сессия недействительна
    s.pool.disconnect(&id).await?;
    Ok(srv)
}

#[tauri::command]
pub async fn delete_server(s: State<'_, AppState>, id: String) -> R<()> {
    s.pool.disconnect(&id).await?;
    s.store.delete(&id)
}

#[tauri::command]
pub async fn test_connection(s: State<'_, AppState>, id: String) -> R<()> {
    let srv = s.store.get(&id)?;
    s.pool.test_connection(&srv, s.store.secrets().as_ref()).await
}

#[tauri::command]
pub async fn connect(s: State<'_, AppState>, id: String) -> R<()> {
    let srv = s.store.get(&id)?;
    s.pool.connect(&srv, s.store.secrets().as_ref()).await
}

#[tauri::command]
pub async fn disconnect(s: State<'_, AppState>, id: String) -> R<()> {
    s.pool.disconnect(&id).await
}

/// Ид подключённых серверов — фронт опрашивает для индикатора статуса.
#[tauri::command]
pub async fn connection_status(s: State<'_, AppState>) -> R<Vec<String>> {
    Ok(s.pool.connected_ids().await)
}

#[tauri::command]
pub async fn exec(s: State<'_, AppState>, id: String, command: String) -> R<ExecResult> {
    let srv = s.store.get(&id)?;
    s.pool.exec(&srv, s.store.secrets().as_ref(), &command).await
}

#[tauri::command]
pub fn trust_host(s: State<AppState>, host: String, port: u16, fingerprint: String) -> R<()> {
    s.pool.trust_host(&host, port, &fingerprint)
}

/// Открывает терминал; вывод идёт в `on_event` (Tauri Channel) байтами.
#[tauri::command]
pub async fn terminal_open(s: State<'_, AppState>, server_id: String, cols: u32, rows: u32, on_event: Channel<TermEvent>) -> R<String> {
    let srv = s.store.get(&server_id)?;
    let (tid, mut rx) = s.pool.terminal_open(&srv, s.store.secrets().as_ref(), cols, rows).await?;
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if on_event.send(ev).is_err() {
                break;
            }
        }
    });
    Ok(tid)
}

#[tauri::command]
pub fn terminal_write(s: State<AppState>, terminal_id: String, data: Vec<u8>) -> R<()> {
    s.pool.terminal_write(&terminal_id, data)
}

#[tauri::command]
pub fn terminal_resize(s: State<AppState>, terminal_id: String, cols: u32, rows: u32) -> R<()> {
    s.pool.terminal_resize(&terminal_id, cols, rows)
}

#[tauri::command]
pub fn terminal_close(s: State<AppState>, terminal_id: String) -> R<()> {
    s.pool.terminal_close(&terminal_id)
}
