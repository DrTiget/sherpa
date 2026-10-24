use serde_json::Value;
pub fn csv(items: &[Value]) -> String {
    let fields = ["id", "timestamp", "client", "connection_id", "method", "outcome"];
    let mut out = format!("{}\r\n", fields.join(","));
    for row in items {
        out.push_str(
            &fields
                .iter()
                .map(|f| cell(row[*f].as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    out
}
fn cell(s: &str) -> String {
    let safe = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_csv() {
        assert_eq!(cell("a,\"b\n"), "\"a,\"\"b\n\"");
        assert_eq!(cell("=cmd"), "\"'=cmd\"");
    }
}
