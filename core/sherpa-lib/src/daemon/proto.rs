//! Сообщения демона (CORE_API §4.2): JSON Lines.
//! Запрос `{id, method, params}`, ответ `{id, ok, data|error}`, событие `{event, …}`.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::CoreError;

/// Максимальная длина строки протокола.
pub const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub fn ok(id: &Value, data: Value) -> Value {
    json!({ "id": id, "ok": true, "data": data })
}

pub fn err(id: &Value, e: &CoreError) -> Value {
    json!({ "id": id, "ok": false, "error": e })
}

/// Событие: `{"event": name, ...fields}`.
pub fn event(name: &str, fields: Value) -> Value {
    let mut m = Map::new();
    m.insert("event".into(), name.into());
    if let Value::Object(f) = fields {
        m.extend(f);
    }
    Value::Object(m)
}

/// Строковый параметр (обязательный).
pub fn req_str(p: &Value, key: &str) -> Result<String, CoreError> {
    match p.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::String(_)) | None | Some(Value::Null) => Err(CoreError::required(key)),
        Some(_) => Err(CoreError::invalid(key, "expected string")),
    }
}

pub fn opt_str(p: &Value, key: &str) -> Result<Option<String>, CoreError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(CoreError::invalid(key, "expected string")),
    }
}

pub fn opt_u64(p: &Value, key: &str) -> Result<Option<u64>, CoreError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| CoreError::invalid(key, "expected non-negative integer")),
    }
}

pub fn opt_bool(p: &Value, key: &str) -> Result<bool, CoreError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(CoreError::invalid(key, "expected boolean")),
    }
}
