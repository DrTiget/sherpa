use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn version(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}
/// Сохраняет номера всех строк, окружение, комментарии и неизвестные расширения.
pub fn crontab(text: &str, system: bool) -> Value {
    let items:Vec<_>=text.lines().enumerate().map(|(i,line)|{
        let trimmed=line.trim();
        let (enabled,body)=if let Some(s)=trimmed.strip_prefix("#SHERPA_DISABLED ") {(false,s)} else {(true,trimmed)};
        let kind=if body.is_empty(){"blank"}else if body.starts_with('#'){"comment"}else if body.split_whitespace().next().is_some_and(|s|s.contains('=')){"environment"}else{"job"};
        let fields=if body.starts_with('@'){1}else{5};
        let mut rest=body;
        let mut schedule=Vec::new();
        for _ in 0..fields {let end=rest.find(char::is_whitespace).unwrap_or(rest.len()); schedule.push(&rest[..end]); rest=rest[end..].trim_start();}
        let user=if system && kind=="job" {let end=rest.find(char::is_whitespace).unwrap_or(rest.len()); let u=Some(&rest[..end]);rest=rest[end..].trim_start();u}else{None};
        json!({"line":i+1,"raw":line,"kind":kind,"enabled":enabled,"schedule":if kind=="job"{Some(schedule.join(" "))}else{None},"command":if kind=="job"{Some(rest)}else{None},"user":user})
    }).collect();
    json!({"items":items,"version":version(text)})
}
pub fn system_files(text: &str) -> CoreResult<Vec<Value>> {
    let mut parts = text.split_terminator('\0');
    let mut items = Vec::new();
    while let Some(path) = parts.next() {
        let content = parts
            .next()
            .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid cron file output"))?;
        let mut value = crontab(content, true);
        value["path"] = json!(path);
        items.push(value);
    }
    Ok(items)
}
pub fn timers(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for part in text.split("\n\n").filter(|s| !s.trim().is_empty()) {
        let mut row = serde_json::Map::new();
        for line in part.lines() {
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid timer output"))?;
            row.insert(k.to_owned(), json!(v));
        }
        if !row.contains_key("Id") {
            return Err(CoreError::new("remote.bad_output", "Missing timer id"));
        }
        items.push(Value::Object(row));
    }
    Ok(json!({"items":items}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_debian() {
        let d = crontab(include_str!("fixtures/crontab-debian.txt"), true);
        assert!(d["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["kind"] == "job" && v["user"] == "root"));
    }
    #[test]
    fn debian_crontab() {
        let d=crontab("# /etc/crontab: system-wide crontab\nSHELL=/bin/sh\n17 * * * * root cd / && run-parts --report /etc/cron.hourly\n@reboot root /usr/local/bin/start\n#SHERPA_DISABLED 0 3 * * * root /bin/backup\n",true);
        assert_eq!(d["items"][1]["kind"], "environment");
        assert_eq!(d["items"][2]["command"], "cd / && run-parts --report /etc/cron.hourly");
        assert_eq!(d["items"][2]["user"], "root");
        assert_eq!(d["items"][4]["enabled"], false);
    }
    #[test]
    fn busybox_and_files() {
        let d = crontab("*/15 * * * * run-parts /etc/periodic/15min\n", false);
        assert_eq!(d["items"][0]["schedule"], "*/15 * * * *");
        assert_eq!(
            system_files("/etc/cron.d/test\0@daily root backup\n\0").unwrap()[0]["path"],
            "/etc/cron.d/test"
        );
        assert!(system_files("broken\0").is_err());
        assert_ne!(version("a"), version("a\n"));
    }
    #[test]
    fn systemd_timer() {
        let d=timers("Id=apt-daily.timer\nActiveState=active\nUnitFileState=enabled\nNextElapseUSecRealtime=Sat 2026-10-24 18:12:00 UTC\n\n").unwrap();
        assert_eq!(d["items"][0]["Id"], "apt-daily.timer");
        assert!(timers("junk").is_err());
    }
}
