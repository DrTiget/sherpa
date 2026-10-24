use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};
fn bad() -> CoreError {
    CoreError::new("remote.bad_output", "Invalid database CLI output")
}
pub fn output(text: &str, engine: &str, queries: bool) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if engine == "redis" {
            if line.starts_with('#') {
                continue;
            }
            if queries {
                let fields: serde_json::Map<String, Value> = line
                    .split_whitespace()
                    .filter_map(|v| v.split_once('='))
                    .map(|(k, v)| (k.into(), json!(v)))
                    .collect();
                if !fields.contains_key("id") {
                    return Err(bad());
                }
                items.push(Value::Object(fields));
            } else {
                let (name, values) = line.split_once(':').ok_or_else(bad)?;
                if !name.starts_with("db") {
                    return Err(bad());
                }
                let mut item = json!({"name":name,"size_bytes":null});
                for pair in values.split(',') {
                    let (k, v) = pair.split_once('=').ok_or_else(bad)?;
                    item[k] = json!(v.parse::<u64>().map_err(|_| bad())?);
                }
                items.push(item);
            }
        } else {
            let fields: Vec<_> = line
                .splitn(
                    if queries {
                        6
                    } else if engine == "postgres" {
                        3
                    } else {
                        2
                    },
                    '\t',
                )
                .collect();
            if queries {
                if fields.len() != 6 {
                    return Err(bad());
                }
                items.push(json!({"id":fields[0].parse::<u64>().map_err(|_|bad())?,"user":fields[1],"database":fields[2],"elapsed_sec":fields[3].parse::<u64>().map_err(|_|bad())?,"state":fields[4],"query":fields[5]}));
            } else {
                if fields.len() < 2 {
                    return Err(bad());
                }
                items.push(json!({"name":fields[0],"size_bytes":fields[1].parse::<u64>().map_err(|_|bad())?,"owner":fields.get(2)}));
            }
        }
    }
    Ok(json!({"items":items}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mysql() {
        assert_eq!(
            output("app\t16384\n", "mysql", false).unwrap()["items"][0]["size_bytes"],
            16384
        );
        assert!(output("garbage", "mysql", false).is_err());
    }
    #[test]
    fn postgres() {
        assert_eq!(
            output("42\tpostgres\tapp\t12\tactive\tselect 1", "postgres", true).unwrap()["items"][0]["elapsed_sec"],
            12
        );
        assert!(output("x\tu\td\t0\tx\tq", "postgres", true).is_err());
    }
    #[test]
    fn redis() {
        assert_eq!(
            output("# Keyspace\r\ndb0:keys=3,expires=1,avg_ttl=1000\r\n", "redis", false).unwrap()["items"][0]["keys"],
            3
        );
        assert_eq!(
            output("id=3 addr=127.0.0.1:1234 cmd=info", "redis", true).unwrap()["items"][0]["id"],
            "3"
        );
    }
}
