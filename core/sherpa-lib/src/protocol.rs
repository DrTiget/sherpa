//! Конверт ответа (CORE_API §1.3–1.6).

use serde_json::{json, Value};

use crate::error::CoreError;

/// Текущая версия протокола.
pub const PROTOCOL: u32 = 1;
/// Минимальная версия протокола, которую ядро ещё поддерживает.
pub const PROTOCOL_MIN: u32 = 1;

/// Предупреждение: не мешает выполнению, фронт может показать его.
pub type Warning = CoreError;

pub fn success(data: Value, warnings: &[Warning]) -> Value {
    let mut v = json!({ "ok": true, "protocol": PROTOCOL, "data": data });
    if !warnings.is_empty() {
        v["warnings"] = serde_json::to_value(warnings).unwrap_or(Value::Null);
    }
    v
}

pub fn failure(err: &CoreError, warnings: &[Warning]) -> Value {
    let mut v = json!({ "ok": false, "protocol": PROTOCOL, "error": err });
    if !warnings.is_empty() {
        v["warnings"] = serde_json::to_value(warnings).unwrap_or(Value::Null);
    }
    v
}
