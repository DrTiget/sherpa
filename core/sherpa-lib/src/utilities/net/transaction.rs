//! Root-owned server-side watchdog: independent of client and daemon lifetime.
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, ExecOutput};

const BASE: &str = "/run/sherpa-net";

fn finish_body(action: &str) -> String {
    let operation = if action == "confirm" {
        "[ \"$state\" = pending ] || exit 76; [ \"$(date +%s)\" -lt \"$(cat \"$d/deadline\")\" ] || exit 76; echo confirmed > \"$d/state\"; rmdir /run/sherpa-net/pending; echo confirmed"
    } else {
        "case \"$state\" in pending|rollback_failed) ;; rolled_back) echo rolled_back; exit 0;; *) exit 76;; esac; if timeout 20 sh \"$d/restore\"; then echo rolled_back > \"$d/state\"; rmdir /run/sherpa-net/pending; echo rolled_back; else echo rollback_failed > \"$d/state\"; exit 77; fi"
    };
    format!("exec 9>\"$d/lock\"; flock -x 9 || exit 77; state=$(cat \"$d/state\") || exit 76; {operation}")
}

pub fn finish_script(id: &str, action: &str) -> String {
    format!(
        "LC_ALL=C; export LC_ALL; d={}; [ -d \"$d\" ] || exit 76; {}",
        shell_quote(&format!("{BASE}/{id}")),
        finish_body(action)
    )
}

pub fn apply_script(backend: &str, id: &str, command: &str, timeout: u64) -> String {
    let (snapshot,restore,tools)=match backend {
        "nft"=>("{ echo 'flush ruleset'; nft list ruleset; } > \"$d/snapshot\"", "nft -f \"$d/snapshot\"", "nft"),
        "iptables"=>("iptables-save > \"$d/snapshot\"", "iptables-restore -w 5 < \"$d/snapshot\"", "iptables iptables-save iptables-restore"),
        _=>("ufw status > \"$d/ufw-status\" && tar -cf \"$d/snapshot\" /etc/ufw /etc/default/ufw", "tar -xf \"$d/snapshot\" -C / || exit $?; if grep -qx 'Status: active' \"$d/ufw-status\"; then ufw reload; else ufw --force disable; fi", "ufw tar grep"),
    };
    let rollback = format!(
        "d={}; {}",
        shell_quote(&format!("{BASE}/{id}")),
        finish_body("rollback")
    );
    let watch=format!("d={}; touch \"$d/armed\"; sleep {timeout}; while :; do [ -d \"$d\" ] || exit 0; sh \"$d/rollback\" && exit 0; state=$(cat \"$d/state\"); case \"$state\" in confirmed|rolled_back) exit 0;; esac; sleep 5; done",shell_quote(&format!("{BASE}/{id}")));
    let restore = format!("d={}; {restore}", shell_quote(&format!("{BASE}/{id}")));
    format!(
        r#"LC_ALL=C; export LC_ALL; umask 077
for t in date timeout flock nohup sh sleep mkdir rmdir cat touch {tools}; do command -v "$t" >/dev/null 2>&1 || exit 127; done
mkdir -p {BASE} || exit 77
mkdir {BASE}/pending 2>/dev/null || exit 75
d={dir}
mkdir "$d" || {{ rmdir {BASE}/pending; exit 77; }}
if ! ( {snapshot} ); then rmdir {BASE}/pending; exit 77; fi
printf '%s\n' {restore} > "$d/restore" || {{ rmdir {BASE}/pending; exit 77; }}
printf '%s\n' {rollback} > "$d/rollback" || {{ rmdir {BASE}/pending; exit 77; }}
printf '%s\n' {watch} > "$d/watch" || {{ rmdir {BASE}/pending; exit 77; }}
echo $(( $(date +%s) + {timeout} )) > "$d/deadline"
echo pending > "$d/state"
nohup sh "$d/watch" </dev/null >"$d/watch.log" 2>&1 &
i=0; while [ ! -f "$d/armed" ]; do i=$((i+1)); if [ "$i" -gt 3 ]; then sh "$d/rollback"; exit 77; fi; sleep 1; done
# Serialize apply with confirm/rollback, including a delayed SSH execution.
( exec 9>"$d/lock"; flock -x 9 || exit 77; [ "$(cat "$d/state")" = pending ] || exit 76; timeout 5 sh -c {quoted_command} )
r=$?; if [ "$r" -ne 0 ]; then sh "$d/rollback"; exit 77; fi
echo pending
"#,
        quoted_command = shell_quote(command),
        dir = shell_quote(&format!("{BASE}/{id}")),
        restore = shell_quote(&restore),
        rollback = shell_quote(&rollback),
        watch = shell_quote(&watch)
    )
}

pub fn check(out: &ExecOutput) -> CoreResult<()> {
    if out.success() {
        return Ok(());
    }
    let (code, message) = match out.exit_status {
        Some(127) => ("cap.missing", "Firewall rollback tools are unavailable"),
        Some(75) => ("net.pending", "Another network transaction is pending"),
        Some(76) => (
            "net.transaction_state",
            "Network transaction is absent or already finished",
        ),
        _ => (
            "net.rollback",
            "Firewall apply or rollback failed; inspect server transaction journal",
        ),
    };
    // Do not expose snapshots or arbitrary remote diagnostic output.
    Err(CoreError::new(code, message)
        .with("capability", "firewall_transaction")
        .with("exit_status", out.exit_status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::{FakeExec, RemoteExec};
    #[test]
    fn watchdog_before_change() {
        let s = apply_script("nft", "test", "nft add rule inet filter input accept", 60);
        assert!(s.find("nohup sh").unwrap() < s.find("nft add rule").unwrap());
        assert!(s.contains("sleep 60"));
        assert!(s.contains("flock -x 9"));
        assert!(s.contains("flush ruleset"));
    }
    #[test]
    fn snapshot_backends() {
        assert!(apply_script("ufw", "test", "true", 10).contains("tar -cf"));
        assert!(apply_script("iptables", "test", "true", 10).contains("iptables-restore -w 5"));
    }
    #[tokio::test]
    async fn fake_transaction() {
        let f = FakeExec::new().on("nohup sh", "pending\n", 0);
        let s = apply_script("nft", "test", "true", 60);
        check(&f.exec(&s).await.unwrap()).unwrap();
        let f = FakeExec::new().on("flock", "", 76);
        assert_eq!(
            check(&f.exec(&finish_script("test", "confirm")).await.unwrap())
                .unwrap_err()
                .code,
            "net.transaction_state"
        );
    }
    #[test]
    fn conflicting_transaction() {
        assert_eq!(
            check(&ExecOutput {
                exit_status: Some(75),
                ..Default::default()
            })
            .unwrap_err()
            .code,
            "net.pending"
        );
    }

    #[cfg(unix)]
    struct Sandbox {
        root: tempfile::TempDir,
    }
    #[cfg(unix)]
    impl Sandbox {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("bin")).unwrap();
            let nft = root.path().join("bin/nft");
            std::fs::write(&nft,"#!/bin/sh\nif [ \"$1\" = list ]; then echo 'table inet filter {}'; elif [ \"$1\" = -f ]; then cp \"$2\" \"$NET_TEST_RESTORED\"; else exit 1; fi\n").unwrap();
            std::fs::set_permissions(nft, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self { root }
        }
        fn run(&self, script: &str) -> std::process::Output {
            let path = format!(
                "{}:{}",
                self.root.path().join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let script = script
                .replace(BASE, &self.root.path().join("transactions").to_string_lossy())
                .replace("fi; sleep 1; done", "fi; sleep 0.01; done");
            std::process::Command::new("sh")
                .arg("-c")
                .arg(script)
                .env("PATH", path)
                .env("NET_TEST_RESTORED", self.root.path().join("restored"))
                .output()
                .unwrap()
        }
        fn state(&self) -> String {
            std::fs::read_to_string(self.root.path().join("transactions/test/state")).unwrap()
        }
    }
    #[test]
    #[cfg(unix)]
    fn shell_watchdog_restores_without_client() {
        let s = Sandbox::new();
        let out = s.run(&apply_script("nft", "test", "true", 2));
        assert!(out.status.success(), "{:?}", out);
        assert_eq!(s.state().trim(), "pending");
        for _ in 0..60 {
            if s.state().trim() == "rolled_back" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(s.state().trim(), "rolled_back");
        let restored = std::fs::read_to_string(s.root.path().join("restored")).unwrap();
        assert!(restored.starts_with("flush ruleset\n"));
        assert_eq!(s.run(&finish_script("test", "confirm")).status.code(), Some(76));
    }
    #[test]
    #[cfg(unix)]
    fn shell_confirmation_and_exclusion() {
        let s = Sandbox::new();
        assert!(s.run(&apply_script("nft", "test", "true", 10)).status.success());
        assert_eq!(s.run(&apply_script("nft", "other", "true", 10)).status.code(), Some(75));
        assert!(s.run(&finish_script("test", "confirm")).status.success());
        assert_eq!(s.state().trim(), "confirmed");
        assert_eq!(s.run(&finish_script("test", "rollback")).status.code(), Some(76));
        assert!(!s.root.path().join("restored").exists());
    }
    #[test]
    #[cfg(unix)]
    fn shell_failed_apply_and_manual_rollback() {
        let s = Sandbox::new();
        assert_eq!(s.run(&apply_script("nft", "test", "false", 10)).status.code(), Some(77));
        assert_eq!(s.state().trim(), "rolled_back");
        assert!(s.run(&finish_script("test", "rollback")).status.success());
        assert!(s.run(&apply_script("nft", "second", "true", 10)).status.success());
        assert!(s.run(&finish_script("second", "rollback")).status.success());
    }
}
