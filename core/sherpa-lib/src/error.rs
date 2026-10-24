//! Единый тип ошибки ядра (CORE_API §1.4, §5).

use serde::Serialize;
use serde_json::{Map, Value};

/// Ошибка ядра: стабильный код, английский текст, параметры для локализации.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoreError {
    pub code: String,
    pub message: String,
    pub params: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<CoreError>,
}

pub type CoreResult<T> = Result<T, CoreError>;

impl CoreError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            params: Map::new(),
            details: Vec::new(),
        }
    }

    /// Добавить параметр для подстановки в локализованный текст.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.params.insert(key.to_string(), value.into());
        self
    }

    pub fn with_details(mut self, details: Vec<CoreError>) -> Self {
        self.details = details;
        self
    }

    pub fn internal(message: impl std::fmt::Display) -> Self {
        let trace_id = uuid::Uuid::now_v7().to_string();
        tracing::error!(%trace_id, "internal error: {message}");
        Self::new("internal", format!("Internal error: {message}")).with("trace_id", trace_id)
    }

    /// Код выхода процесса по CORE_API §1.5.
    pub fn exit_code(&self) -> i32 {
        let c = self.code.as_str();
        if c == "store.schema_too_new" || c.starts_with("protocol.") {
            4
        } else if c.starts_with("request.") {
            2
        } else if c.starts_with("store.") || c.starts_with("crypto.") || c.starts_with("config.") {
            3
        } else if c == "internal" {
            70
        } else {
            1
        }
    }

    // --- частые конструкторы ---

    pub fn invalid_args(arg: &str, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::new("request.invalid_args", format!("Invalid argument {arg}: {reason}"))
            .with("arg", arg)
            .with("reason", reason)
    }

    pub fn required(field: &str) -> Self {
        Self::new("validation.required", format!("Field is required: {field}")).with("field", field)
    }

    pub fn invalid(field: &str, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::new("validation.invalid", format!("Invalid value of {field}: {reason}"))
            .with("field", field)
            .with("reason", reason)
    }

    pub fn too_long(field: &str, max: usize) -> Self {
        Self::new("validation.too_long", format!("Field {field} is longer than {max}"))
            .with("field", field)
            .with("max", max)
    }

    pub fn fs(code: &str, path: &std::path::Path, err: &std::io::Error) -> Self {
        Self::new(code, format!("{code}: {}: {err}", path.display()))
            .with("path", path.display().to_string())
            .with("reason", err.to_string())
    }
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CoreError {}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        if let rusqlite::Error::SqliteFailure(f, _) = &e {
            match f.code {
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                    return CoreError::new("store.locked", "Database is locked");
                }
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                    return CoreError::new("store.corrupted", "Database file is corrupted");
                }
                _ => {}
            }
        }
        CoreError::internal(format!("sqlite: {e}"))
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::internal(format!("json: {e}"))
    }
}
