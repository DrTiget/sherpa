//! Разбор systemctl show и journalctl -o json (systemd 252/255).
use crate::error::CoreResult;
use crate::utilities::management::bad;
use serde_json::{json, Map, Value};

pub fn units(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut item = Map::new();
        for line in block.lines() {
            let (k, v) = line.split_once('=').ok_or_else(|| bad("systemctl show"))?;
            match k {
                "Id" => {
                    item.insert("name".into(), v.into());
                }
                "Description" => {
                    item.insert("description".into(), v.into());
                }
                "LoadState" => {
                    item.insert("load_state".into(), v.into());
                }
                "ActiveState" => {
                    item.insert("active_state".into(), v.into());
                }
                "SubState" => {
                    item.insert("sub_state".into(), v.into());
                }
                "UnitFileState" => {
                    item.insert("enabled".into(), v.into());
                }
                "User" => {
                    item.insert("user".into(), v.into());
                }
                "MainPID" | "MemoryCurrent" => {
                    let key = if k == "MainPID" { "pid" } else { "memory_bytes" };
                    let value = if v == "[not set]" || v == "infinity" || v == "18446744073709551615" {
                        Value::Null
                    } else {
                        v.parse::<u64>().map(Value::from).map_err(|_| bad("systemctl show"))?
                    };
                    item.insert(key.into(), value);
                }
                _ => {}
            }
        }
        if item.get("name").and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(bad("systemctl show"));
        }
        items.push(Value::Object(item));
    }
    Ok(json!({"items":items}))
}

pub fn logs(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        let raw: Value = serde_json::from_str(line).map_err(|_| bad("journalctl"))?;
        if !raw.is_object() {
            return Err(bad("journalctl"));
        }
        let timestamp = raw
            .get("__REALTIME_TIMESTAMP")
            .and_then(Value::as_str)
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| bad("journalctl"))?;
        items.push(json!({"timestamp_usec":timestamp,"message":raw.get("MESSAGE"),
            "priority":raw.get("PRIORITY").and_then(Value::as_str).and_then(|s| s.parse::<u8>().ok()),
            "unit":raw.get("_SYSTEMD_UNIT"),"identifier":raw.get("SYSLOG_IDENTIFIER")}));
    }
    Ok(json!({"items":items}))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub const SHOW: &str = "Id=nginx.service\nDescription=A high performance web server and a reverse proxy server\nLoadState=loaded\nActiveState=active\nSubState=running\nUnitFileState=enabled\nMainPID=812\nMemoryCurrent=6291456\nUser=\n\nId=ssh.service\nDescription=OpenBSD Secure Shell server\nActiveState=inactive\nMemoryCurrent=[not set]\n";
    #[test]
    fn show() {
        let v = units(SHOW).unwrap();
        assert_eq!(v["items"][0]["memory_bytes"], 6291456);
        assert!(v["items"][1]["memory_bytes"].is_null());
    }
    #[test]
    fn journal() {
        let v=logs(r#"{"__REALTIME_TIMESTAMP":"1729761302000000","PRIORITY":"6","_SYSTEMD_UNIT":"nginx.service","MESSAGE":"Started A high performance web server."}"#).unwrap();
        assert_eq!(v["items"][0]["priority"], 6);
    }
    #[test]
    fn invalid() {
        assert!(units("junk").is_err());
        assert!(logs("{}").is_err());
        assert!(logs("garbage").is_err());
        assert_eq!(units("").unwrap()["items"], json!([]));
    }
}
