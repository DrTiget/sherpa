#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::Value;

/// Изолированное окружение: свои конфиг, БД, ключ-файл и HOME.
pub struct Env {
    pub dir: tempfile::TempDir,
}

impl Env {
    pub fn new() -> Self {
        Env {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    /// Окружение с заданным `core.toml` и выполненным `init`.
    pub fn with_config(toml: &str) -> Self {
        let e = Env::new();
        std::fs::write(e.path("core.toml"), toml).unwrap();
        let (v, code) = e.run(&["init"]);
        assert_eq!(code, 0, "{v}");
        e
    }

    pub fn initialized() -> Self {
        let e = Env::new();
        let (v, code) = e.run(&["init"]);
        assert_eq!(code, 0, "{v}");
        e
    }

    pub fn path(&self, name: &str) -> std::path::PathBuf {
        self.dir.path().join(name)
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("sherpa-core").unwrap();
        c.env("SHERPA_CONFIG", self.path("core.toml"))
            .env("SHERPA_DB", self.path("sherpa.db"))
            .env("SHERPA_KEY_FILE", self.path("sherpa.key"))
            .env("HOME", self.dir.path())
            .env("USERPROFILE", self.dir.path())
            .env("XDG_RUNTIME_DIR", self.dir.path().join("run"))
            .env_remove("SHERPA_LOG");
        c
    }

    fn output(&self, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
        let mut c = self.cmd();
        c.args(args);
        if let Some(s) = stdin {
            c.args(["--input", "-"]).write_stdin(s.to_string());
        }
        let o = c.output().unwrap();
        (
            String::from_utf8(o.stdout).unwrap(),
            String::from_utf8(o.stderr).unwrap(),
            o.status.code().unwrap_or(-1),
        )
    }

    /// Выполнить и вернуть (единственный JSON-ответ, код выхода).
    pub fn run(&self, args: &[&str]) -> (Value, i32) {
        self.run_in(args, None)
    }

    pub fn run_in(&self, args: &[&str], stdin: Option<&str>) -> (Value, i32) {
        let (out, err, code) = self.output(args, stdin);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "expected exactly one JSON line, got: {out}\nstderr: {err}"
        );
        (serde_json::from_str(lines[0]).unwrap(), code)
    }

    /// Успешный вызов → `data`.
    pub fn ok(&self, args: &[&str]) -> Value {
        self.ok_in(args, None)
    }

    pub fn ok_in(&self, args: &[&str], stdin: Option<&str>) -> Value {
        let (v, code) = self.run_in(args, stdin);
        assert_eq!(code, 0, "{v}");
        assert_eq!(v["ok"], true);
        assert_eq!(v["protocol"], 1);
        v["data"].clone()
    }

    /// Ошибка → (code, params, exit).
    pub fn err(&self, args: &[&str]) -> (String, Value, i32) {
        self.err_in(args, None)
    }

    pub fn err_in(&self, args: &[&str], stdin: Option<&str>) -> (String, Value, i32) {
        let (v, code) = self.run_in(args, stdin);
        assert_eq!(v["ok"], false, "{v}");
        assert!(v["error"]["message"].is_string());
        (
            v["error"]["code"].as_str().unwrap().to_string(),
            v["error"]["params"].clone(),
            code,
        )
    }

    /// Все строки JSON Lines (для --stream).
    pub fn lines(&self, args: &[&str]) -> (Vec<Value>, i32) {
        let (out, _, code) = self.output(args, None);
        (out.lines().map(|l| serde_json::from_str(l).unwrap()).collect(), code)
    }

    pub fn create(&self, json: &str) -> String {
        let d = self.ok_in(&["conn", "create"], Some(json));
        d["connection"]["id"].as_str().unwrap().to_string()
    }

    pub fn stderr_of(&self, args: &[&str], stdin: Option<&str>) -> String {
        self.output(args, stdin).1
    }
}
