use crate::error::{CoreError, CoreResult};
use serde_json::Value;
pub fn snapshots(text: &str) -> CoreResult<Value> {
    let v: Value =
        serde_json::from_str(text).map_err(|_| CoreError::new("remote.bad_output", "Invalid restic snapshots JSON"))?;
    let a = v
        .as_array()
        .ok_or_else(|| CoreError::new("remote.bad_output", "Expected snapshots array"))?;
    if a.iter().any(|v| !v["id"].is_string() || !v["time"].is_string()) {
        return Err(CoreError::new("remote.bad_output", "Invalid snapshot"));
    }
    Ok(serde_json::json!({"items":a}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restic_json() {
        let v=snapshots(r#"[{"time":"2026-10-24T03:00:00Z","parent":"abc","tree":"123","paths":["/srv"],"hostname":"web","username":"root","id":"e9a8","short_id":"e9a8"}]"#).unwrap();
        assert_eq!(v["items"][0]["paths"][0], "/srv");
        assert!(snapshots("{}").is_err());
        assert!(snapshots("garbage").is_err());
    }
}

/// Только итоговые счётчики: пути и диагностические строки наружу не уходят.
pub fn run(engine: &str, text: &str) -> CoreResult<Value> {
    if engine == "restic" {
        for line in text.lines().rev() {
            let v: Value = serde_json::from_str(line)
                .map_err(|_| CoreError::new("remote.bad_output", "Invalid restic progress JSON"))?;
            if v["message_type"] == "summary" {
                let bytes = v["total_bytes_processed"]
                    .as_u64()
                    .ok_or_else(|| CoreError::new("remote.bad_output", "Missing restic summary counters"))?;
                return Ok(
                    serde_json::json!({"success":true,"processed_bytes":bytes,"added_bytes":v["data_added"].as_u64(),"snapshot_id":v["snapshot_id"],"dry_run":false}),
                );
            }
        }
    } else if engine == "rsync" {
        let bytes = text
            .lines()
            .find_map(|l| l.strip_prefix("Total transferred file size:"))
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.replace(',', "").parse::<u64>().ok());
        if let Some(bytes) = bytes {
            return Ok(serde_json::json!({"success":true,"processed_bytes":bytes,"dry_run":false}));
        }
    }
    Err(CoreError::new("remote.bad_output", "Missing backup summary"))
}
#[cfg(test)]
mod run_tests {
    use super::*;
    #[test]
    fn summaries() {
        assert_eq!(
            run(
                "restic",
                r#"{"message_type":"summary","total_bytes_processed":1024,"data_added":128,"snapshot_id":"abcd"}"#
            )
            .unwrap()["processed_bytes"],
            1024
        );
        assert_eq!(
            run(
                "rsync",
                "Number of files: 4\nTotal transferred file size: 1,024 bytes\n"
            )
            .unwrap()["processed_bytes"],
            1024
        );
        assert!(run("restic", "success").is_err());
    }
}
