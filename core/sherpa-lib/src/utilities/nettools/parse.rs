//! Bounded diagnostic spool framing, independent of SSH.
use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};

pub const MAX_OUTPUT: usize = 65_536;

pub fn frame(text: &str) -> CoreResult<(Option<u32>, &str)> {
    let (status, output) = text
        .split_once('\n')
        .ok_or_else(|| CoreError::new("remote.bad_output", "Missing diagnostic status"))?;
    let code = if status == "running" {
        None
    } else {
        Some(
            status
                .parse::<u32>()
                .map_err(|_| CoreError::new("remote.bad_output", "Invalid diagnostic exit status"))?,
        )
    };
    Ok((code, output))
}

pub fn records(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| json!({"text":line.trim_end_matches('\r')}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ping_linux_busybox() {
        for s in ["running\nPING example.org (192.0.2.1) 56(84) bytes of data.\n64 bytes from 192.0.2.1: icmp_seq=1 ttl=64 time=0.052 ms\n","0\nPING example.org (192.0.2.1): 56 data bytes\n64 bytes from 192.0.2.1: seq=0 ttl=64 time=0.052 ms\n"] {let (_,out)=frame(s).unwrap();assert_eq!(records(out).len(),2);}
    }
    #[test]
    fn headers_and_errors() {
        assert_eq!(records("HTTP/2 200\r\nserver: nginx\r\n")[0]["text"], "HTTP/2 200");
        assert!(frame("bad\n").is_err());
        assert!(frame("").is_err());
        assert_eq!(frame("124\n").unwrap().0, Some(124));
    }
}
