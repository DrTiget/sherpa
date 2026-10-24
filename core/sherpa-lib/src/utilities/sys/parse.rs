//! Форматы getent (glibc/musl), timedatectl show и uname.
use crate::error::CoreResult;
use crate::utilities::management::bad;
use serde_json::{json, Map, Value};
pub fn properties(text: &str) -> CoreResult<Value> {
    let mut data = Map::new();
    for line in text.lines().filter(|s| !s.is_empty()) {
        let (key, value) = line.split_once('=').ok_or_else(|| bad("properties"))?;
        data.insert(key.into(), value.into());
    }
    if data.is_empty() {
        return Err(bad("properties"));
    }
    Ok(Value::Object(data))
}
pub fn time(text: &str) -> CoreResult<Value> {
    let p = properties(text)?;
    let zone = p["Timezone"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("timedatectl"))?;
    let boolean = |k: &str| -> CoreResult<Option<bool>> {
        match p.get(k).and_then(Value::as_str) {
            Some("yes") => Ok(Some(true)),
            Some("no") => Ok(Some(false)),
            None => Ok(None),
            _ => Err(bad("timedatectl")),
        }
    };
    Ok(
        json!({"timezone":zone,"time":p.get("TimeUSec"),"rtc_time":p.get("RTCTimeUSec"),"ntp":boolean("NTP")?,"ntp_synchronized":boolean("NTPSynchronized")?,"can_ntp":boolean("CanNTP")?}),
    )
}
pub fn groups(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines().filter(|s| !s.is_empty()) {
        let f: Vec<_> = line.split(':').collect();
        if f.len() != 4 || f[0].is_empty() {
            return Err(bad("getent group"));
        }
        let gid = f[2].parse::<u32>().map_err(|_| bad("getent group"))?;
        let members: Vec<_> = f[3].split(',').filter(|s| !s.is_empty()).collect();
        items.push(json!({"name":f[0],"gid":gid,"members":members}));
    }
    Ok(json!({"items":items}))
}
pub fn users(text: &str) -> CoreResult<Value> {
    let (passwd, group) = text.split_once("@@SHERPA@@\n").ok_or_else(|| bad("getent passwd"))?;
    let groups = groups(group)?;
    let mut items = Vec::new();
    for line in passwd.lines().filter(|s| !s.is_empty()) {
        let f: Vec<_> = line.split(':').collect();
        if f.len() != 7 || f[0].is_empty() {
            return Err(bad("getent passwd"));
        }
        let uid = f[2].parse::<u32>().map_err(|_| bad("getent passwd"))?;
        let gid = f[3].parse::<u32>().map_err(|_| bad("getent passwd"))?;
        let memberships: Vec<_> = groups["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| g["gid"] == gid || g["members"].as_array().is_some_and(|m| m.iter().any(|u| u == f[0])))
            .map(|g| g["name"].clone())
            .collect();
        items.push(
            json!({"name":f[0],"uid":uid,"gid":gid,"description":f[4],"home":f[5],"shell":f[6],"groups":memberships}),
        );
    }
    Ok(json!({"items":items}))
}
pub fn info(text: &str) -> CoreResult<Value> {
    let parts: Vec<_> = text.trim_end().split('\n').collect();
    if parts.len() != 4 || parts.iter().any(|s| s.is_empty()) {
        return Err(bad("sys.info"));
    }
    Ok(json!({"hostname":parts[0],"kernel":parts[1],"kernel_release":parts[2],"arch":parts[3]}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounts_debian() {
        let p="root:x:0:0:root:/root:/bin/bash\ndeploy:x:1000:1000:Deploy:/home/deploy:/bin/bash\n@@SHERPA@@\nroot:x:0:\nsudo:x:27:deploy\ndeploy:x:1000:\n";
        let v = users(p).unwrap();
        assert_eq!(v["items"][1]["groups"], json!(["sudo", "deploy"]));
        assert!(v.to_string().find(":x:").is_none());
    }
    #[test]
    fn accounts_alpine() {
        let v = users("root:x:0:0:root:/root:/bin/ash\n@@SHERPA@@\nroot:x:0:root\nwheel:x:10:root\n").unwrap();
        assert_eq!(v["items"][0]["shell"], "/bin/ash");
    }
    #[test]
    fn clock() {
        let v = time(
            "Timezone=Europe/Moscow\nNTP=yes\nNTPSynchronized=no\nCanNTP=yes\nTimeUSec=Sat 2026-10-24 12:00:00 MSK\n",
        )
        .unwrap();
        assert_eq!(v["ntp"], true);
        assert_eq!(v["ntp_synchronized"], false);
    }
    #[test]
    fn malformed() {
        for p in ["garbage", "a:x:no:0::/:/bin/sh\n@@SHERPA@@\n"] {
            assert!(users(p).is_err());
        }
        assert!(time("NTP=maybe").is_err());
        assert!(groups("a:x:no:").is_err());
        assert!(info("junk").is_err());
    }
}
