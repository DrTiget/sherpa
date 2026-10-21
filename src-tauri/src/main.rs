#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;

use servergui_core::{known_hosts::KnownHosts, secrets::KeyringStore, ssh::SshPool, store::ServerStore};
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub store: ServerStore,
    pub pool: SshPool,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState {
                store: ServerStore::new(dir.join("servers.json"), Arc::new(KeyringStore::new("dev.servergui.app"))),
                pool: SshPool::new(KnownHosts::new(dir.join("known_hosts"))),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::create_server,
            commands::update_server,
            commands::delete_server,
            commands::test_connection,
            commands::connect,
            commands::disconnect,
            commands::connection_status,
            commands::exec,
            commands::trust_host,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска приложения");
}
