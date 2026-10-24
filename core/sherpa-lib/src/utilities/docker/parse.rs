use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};
pub fn objects(text: &str) -> CoreResult<Value> {
    let bad = || CoreError::new("remote.bad_output", "Invalid Docker JSON output");
    let mut items = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let value: Value = serde_json::from_str(line).map_err(|_| bad())?;
        match value {
            Value::Object(_) => items.push(value),
            Value::Array(v) if v.iter().all(Value::is_object) => items.extend(v),
            _ => return Err(bad()),
        }
    }
    Ok(json!({"items":items}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats() {
        assert_eq!(
            objects("{\"Name\":\"bridge\",\"Driver\":\"bridge\"}\n").unwrap()["items"][0]["Name"],
            "bridge"
        );
        assert_eq!(objects("[]").unwrap()["items"], json!([]));
        assert!(objects("noise").is_err());
        assert!(objects("42").is_err());
        assert!(objects("[1]").is_err());
    }
}

fn bytes(text: &str) -> CoreResult<u64> {
    let text = text.trim();
    let pos = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let number = text[..pos]
        .parse::<f64>()
        .map_err(|_| CoreError::new("remote.bad_output", "Invalid Docker size"))?;
    let multiplier = match text[pos..].trim() {
        "B" | "" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KiB" => 1024.0,
        "MiB" => 1048576.0,
        "GiB" => 1073741824.0,
        "TiB" => 1099511627776.0,
        _ => return Err(CoreError::new("remote.bad_output", "Invalid Docker size unit")),
    };
    let value = number * multiplier;
    if !value.is_finite() || value < 0.0 || value > u64::MAX as f64 {
        return Err(CoreError::new("remote.bad_output", "Invalid Docker size range"));
    }
    Ok(value as u64)
}
pub fn stats(text: &str) -> CoreResult<Value> {
    let raw = objects(text)?;
    let mut items = Vec::new();
    for item in raw["items"].as_array().into_iter().flatten() {
        let field = |k: &str| {
            item[k]
                .as_str()
                .ok_or_else(|| CoreError::new("remote.bad_output", "Missing Docker stats field").with("field", k))
        };
        let cpu = field("CPUPerc")?
            .trim_end_matches('%')
            .parse::<f64>()
            .map_err(|_| CoreError::new("remote.bad_output", "Invalid Docker CPU percentage"))?;
        if !cpu.is_finite() || cpu < 0.0 {
            return Err(CoreError::new("remote.bad_output", "Invalid Docker CPU percentage"));
        }
        let (used, total) = field("MemUsage")?
            .split_once('/')
            .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid Docker memory usage"))?;
        items.push(json!({"id":item["ID"],"name":item["Name"],"cpu_pct":cpu,"memory_used_bytes":bytes(used)?,"memory_limit_bytes":bytes(total)?}));
    }
    Ok(json!({"items":items}))
}
#[cfg(test)]
mod stats_tests {
    use super::*;
    #[test]
    fn docker_stats() {
        let d = stats(r#"{"ID":"abc","Name":"web","CPUPerc":"0.50%","MemUsage":"12.5MiB / 1GiB"}"#).unwrap();
        assert_eq!(d["items"][0]["memory_used_bytes"], 13107200);
        assert_eq!(d["items"][0]["cpu_pct"], 0.5);
        assert!(stats("{}").is_err());
        assert!(bytes("1watts").is_err());
    }
}
