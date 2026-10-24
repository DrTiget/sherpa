//! Чистые парсеры util-linux/coreutils. Имена файлов разделяются NUL.
use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};

pub fn bad() -> CoreError {
    CoreError::new("remote.bad_output", "Unexpected disk output").with("command", "disk")
}

pub fn df(text: &str) -> CoreResult<Vec<Value>> {
    let mut lines = text.lines();
    if !lines.next().is_some_and(|s| s.starts_with("Filesystem")) {
        return Err(bad());
    }
    lines.map(|line| {
        let p: Vec<_> = line.split_whitespace().collect();
        if p.len() < 6 { return Err(bad()); }
        let n = |i: usize| -> CoreResult<Value> { if p[i] == "-" { Ok(Value::Null) } else { p[i].trim_end_matches('%').parse::<u64>().map(Value::from).map_err(|_| bad()) } };
        Ok(json!({"device":p[0], "total_bytes":n(1)?, "used_bytes":n(2)?, "available_bytes":n(3)?, "usage_pct":n(4)?, "mount":p[5..].join(" ")}))
    }).collect()
}

pub fn list(text: &str) -> CoreResult<Value> {
    let p: Vec<_> = text.split("@@SHERPA_DISK@@\n").collect();
    if p.len() != 4 {
        return Err(bad());
    }
    let blocks: Value = serde_json::from_str(p[1]).map_err(|_| bad())?;
    let mounts: Value = serde_json::from_str(p[2]).map_err(|_| bad())?;
    if !blocks["blockdevices"].is_array() || !mounts["filesystems"].is_array() {
        return Err(bad());
    }
    let mut items = df(p[0])?;
    let inodes = df(p[3])?;
    for item in &mut items {
        if let Some(i) = inodes.iter().find(|i| i["mount"] == item["mount"]) {
            item["inodes_total"] = i["total_bytes"].clone();
            item["inodes_used"] = i["used_bytes"].clone();
            item["inodes_available"] = i["available_bytes"].clone();
            item["inodes_usage_pct"] = i["usage_pct"].clone();
        }
    }
    Ok(json!({"items":items, "blockdevices":blocks["blockdevices"], "filesystems":mounts["filesystems"]}))
}

pub fn sizes(text: &str, large: bool) -> CoreResult<Vec<Value>> {
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(bad());
    }
    text.split_terminator('\0')
        .map(|s| {
            let (size, path) = s.split_once('\t').ok_or_else(bad)?;
            let size: u64 = size.parse().map_err(|_| bad())?;
            Ok(json!({"path":path, "size_bytes":if large {size} else {size.checked_mul(1024).ok_or_else(bad)?}}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_debian() {
        assert_eq!(df(include_str!("fixtures/df-debian.txt")).unwrap()[0]["mount"], "/");
        let blocks: Value = serde_json::from_str(include_str!("fixtures/lsblk-debian.json")).unwrap();
        assert!(blocks["blockdevices"].is_array());
    }
    #[test]
    fn debian_df() {
        let d = df(
            "Filesystem 1B-blocks Used Available Use% Mounted on\n/dev/vda1 21474836480 1073741824 19327352832 6% /\n",
        )
        .unwrap();
        assert_eq!(d[0]["available_bytes"], 19327352832u64);
    }
    #[test]
    fn busybox_df() {
        assert_eq!(
            df("Filesystem 1B-blocks Used Available Use% Mounted on\noverlay 1024 512 512 50% /\n").unwrap()[0]
                ["usage_pct"],
            50
        );
    }
    #[test]
    fn paths_and_overflow() {
        assert_eq!(sizes("12\t/a\nb\0", false).unwrap()[0]["size_bytes"], 12288);
        assert_eq!(sizes("13\t/a\tb\0", true).unwrap()[0]["path"], "/a\tb");
        assert!(sizes("18446744073709551615\t/a\0", false).is_err());
        assert!(sizes("junk", false).is_err());
        assert!(df("junk").is_err());
    }
    #[test]
    fn util_linux_json() {
        let d = list("Filesystem 1B-blocks Used Available Use% Mounted on\n@@SHERPA_DISK@@\n{\"blockdevices\":[{\"name\":\"vda\",\"size\":21474836480}]}\n@@SHERPA_DISK@@\n{\"filesystems\":[{\"target\":\"/\",\"fstype\":\"ext4\"}]}\n@@SHERPA_DISK@@\nFilesystem Inodes IUsed IFree IUse% Mounted on\n").unwrap();
        assert_eq!(d["filesystems"][0]["fstype"], "ext4");
        assert!(list("junk").is_err());
    }
}
