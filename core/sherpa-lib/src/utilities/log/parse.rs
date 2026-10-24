use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};

pub fn journal(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    let mut cursor = None;
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        let fields: Value = serde_json::from_str(line)
            .map_err(|_| CoreError::new("remote.bad_output", "Invalid journal JSON").with("command", "journalctl"))?;
        if !fields.is_object() {
            return Err(CoreError::new("remote.bad_output", "Expected journal object").with("command", "journalctl"));
        }
        let str_value = |key: &str| fields[key].as_str().map(str::to_owned);
        cursor = str_value("__CURSOR");
        items.push(json!({"timestamp_usec":str_value("__REALTIME_TIMESTAMP").and_then(|s|s.parse::<u64>().ok()),"cursor":cursor,"priority":str_value("PRIORITY").and_then(|s|s.parse::<u8>().ok()),"unit":fields["_SYSTEMD_UNIT"],"message":fields["MESSAGE"],"fields":fields}));
    }
    Ok(json!({"items":items,"cursor":cursor}))
}
pub fn paths(text: &str) -> Value {
    json!({"files":text.split_terminator('\0').collect::<Vec<_>>()})
}
pub fn tail(text: &str) -> Value {
    json!({"lines":text.lines().collect::<Vec<_>>()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debian_journal() {
        let d=journal("{\"__CURSOR\":\"s=abc;i=1\",\"__REALTIME_TIMESTAMP\":\"1729760400123456\",\"PRIORITY\":\"6\",\"_SYSTEMD_UNIT\":\"ssh.service\",\"MESSAGE\":\"Started OpenBSD Secure Shell server.\"}\n").unwrap();
        assert_eq!(d["items"][0]["timestamp_usec"], 1729760400123456u64);
        assert_eq!(d["items"][0]["priority"], 6);
        assert_eq!(d["cursor"], "s=abc;i=1");
    }
    #[test]
    fn empty_binary_and_bad() {
        assert_eq!(journal("").unwrap()["items"], json!([]));
        assert_eq!(
            journal("{\"MESSAGE\":[0,255]}").unwrap()["items"][0]["message"],
            json!([0, 255])
        );
        assert!(journal("garbage").is_err());
        assert!(journal("[]").is_err());
        assert_eq!(paths("/var/log/a b\0")["files"], json!(["/var/log/a b"]));
        assert_eq!(tail("one\ntwo\n")["lines"], json!(["one", "two"]));
    }
}
