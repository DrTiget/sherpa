//! Общий SSH-слой ядра: подключение, проверка host key (TOFU), авторизация,
//! выполнение команд. Его используют `conn check`, а дальше — демон и все
//! утилиты рабочего стола (fs, monitor, disk, net, log, docker…).

pub mod client;
pub mod exec;
pub mod keys;
pub mod pool;
pub mod target;

pub use client::{connect, ConnectOptions, ConnectOutcome, Session};
pub use exec::{shell_quote, ExecOutput, FakeExec, RemoteExec};
pub use pool::{Pool, PooledConn};
pub use target::{AuthMaterial, KeySource, Target};
