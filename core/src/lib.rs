//! Ядро Sherpa: модель серверов, хранение, known_hosts и SSH-сессии.
//! Не зависит от Tauri, поэтому собирается и тестируется без системных GUI-библиотек.
pub mod enhanced;
pub mod error;
pub mod known_hosts;
pub mod model;
pub mod monitor;
pub mod secrets;
pub mod ssh;
pub mod store;
pub mod terminal;

pub use error::{CoreError, CoreResult};
