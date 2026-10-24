use crate::ssh::ExecOutput;
use serde_json::{json, Value};
pub fn result(out: &ExecOutput, elapsed_ms: u128) -> Value {
    json!({"exit_status":out.exit_status,"stdout":out.stdout_lossy(),"stderr":out.stderr_lossy(),"elapsed_ms":elapsed_ms})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonzero_is_result() {
        let v = result(
            &ExecOutput {
                stdout: b"ok\n".to_vec(),
                stderr: b"failure\n".to_vec(),
                exit_status: Some(2),
            },
            12,
        );
        assert_eq!(v["exit_status"], 2);
        assert_eq!(v["elapsed_ms"], 12);
    }
}
