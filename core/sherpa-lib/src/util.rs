//! Мелкие общие функции: время, идентификаторы, разбор JSON-параметров.

use serde_json::{Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{CoreError, CoreResult};

/// Текущее время UTC в RFC 3339 с точностью до секунды.
pub fn now() -> String {
    let t = OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    t.format(&Rfc3339).unwrap_or_default()
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

/// Наложить объект `top` поверх `base` (stdin перекрывает флаги, §1.1).
pub fn merge(base: Value, top: Option<Value>) -> CoreResult<Value> {
    let mut base = match base {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    match top {
        None => {}
        Some(Value::Object(m)) => {
            for (k, v) in m {
                base.insert(k, v);
            }
        }
        Some(_) => {
            return Err(CoreError::new("request.invalid_json", "stdin must contain a JSON object").with("offset", 0));
        }
    }
    Ok(Value::Object(base))
}

/// Объект только с непустыми значениями (для флагов, которых не было).
pub fn obj(pairs: Vec<(&str, Option<Value>)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        if let Some(v) = v {
            m.insert(k.to_string(), v);
        }
    }
    Value::Object(m)
}

/// Типизированный разбор параметров; ошибки serde — validation.invalid.
pub fn parse<T: serde::de::DeserializeOwned>(v: Value) -> CoreResult<T> {
    serde_json::from_value(v).map_err(|e| {
        let msg = e.to_string();
        let field = msg.split('`').nth(1).unwrap_or("input").to_string();
        CoreError::invalid(&field, msg)
    })
}

pub fn opt_str(v: &Value, field: &str) -> CoreResult<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(CoreError::invalid(field, "expected string or null")),
    }
}
