//! Разбор `~/.ssh/config` для импорта (ТЗ §4.4, CORE_API §3.9).
//! Поддерживаются Host, HostName, Port, User, IdentityFile, Include.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};

pub const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Debug, Clone)]
struct Line {
    keyword: String,
    args: Vec<String>,
    file: PathBuf,
    line: usize,
}

#[derive(Debug)]
struct Block {
    /// None — строки до первого Host (применяются ко всем).
    patterns: Option<Vec<String>>,
    header: String,
    file: PathBuf,
    line: usize,
    is_match: bool,
    opts: Vec<Line>,
}

/// Кандидат на импорт.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub key: String,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub identity_file: Option<String>,
    pub file: PathBuf,
    pub line: usize,
    pub defaults_from: Vec<String>,
    pub warnings: Vec<CoreError>,
    /// Повтор алиаса в файлах — второй и далее пропускаются.
    pub duplicate_in_file: bool,
}

#[derive(Debug)]
pub struct Parsed {
    pub source: PathBuf,
    pub files: Vec<PathBuf>,
    pub candidates: Vec<Candidate>,
    pub ignored: Vec<Value>,
    pub warnings: Vec<CoreError>,
    /// Отпечаток содержимого всех файлов (preview_id).
    pub digest: String,
}

pub fn default_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".ssh").join("config")
}

fn tokenize(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut has = false;
    for ch in rest.chars() {
        match ch {
            '"' => {
                in_q = !in_q;
                has = true;
            }
            c if c.is_whitespace() && !in_q => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

fn split_line(raw: &str) -> Option<(String, Vec<String>)> {
    let t = raw.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let end = t.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(t.len());
    let keyword = t[..end].to_lowercase();
    let rest = t[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    Some((keyword, tokenize(rest)))
}

struct Reader {
    home: PathBuf,
    files: Vec<PathBuf>,
    hasher: Sha256,
    lines: Vec<Line>,
    warnings: Vec<CoreError>,
}

impl Reader {
    fn read(&mut self, path: &Path, depth: usize, stack: &mut Vec<PathBuf>, required: bool) -> CoreResult<()> {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if stack.contains(&canon) {
            self.warnings.push(
                CoreError::new("import.include_cycle", "Include cycle detected")
                    .with("file", path.display().to_string()),
            );
            return Ok(());
        }
        if depth > MAX_INCLUDE_DEPTH {
            self.warnings.push(
                CoreError::new("import.include_too_deep", "Include nesting is too deep")
                    .with("file", path.display().to_string())
                    .with("max", MAX_INCLUDE_DEPTH),
            );
            return Ok(());
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if required && e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CoreError::new("fs.not_found", "File not found").with("path", path.display().to_string()))
            }
            Err(e) if required => return Err(CoreError::fs("fs.read_failed", path, &e)),
            Err(e) => {
                self.warnings.push(CoreError::fs("fs.read_failed", path, &e));
                return Ok(());
            }
        };
        self.hasher.update(path.display().to_string().as_bytes());
        self.hasher.update([0]);
        self.hasher.update(text.as_bytes());
        self.hasher.update([0]);
        if !self.files.contains(&path.to_path_buf()) {
            self.files.push(path.to_path_buf());
        }
        stack.push(canon);
        for (i, raw) in text.lines().enumerate() {
            let Some((keyword, args)) = split_line(raw) else {
                continue;
            };
            if keyword == "include" {
                for pat in &args {
                    for p in self.expand_include(pat) {
                        self.read(&p, depth + 1, stack, false)?;
                    }
                }
                continue;
            }
            self.lines.push(Line {
                keyword,
                args,
                file: path.to_path_buf(),
                line: i + 1,
            });
        }
        stack.pop();
        Ok(())
    }

    fn expand_include(&self, pat: &str) -> Vec<PathBuf> {
        let expanded = expand_home(pat, &self.home);
        let full = if Path::new(&expanded).is_absolute() {
            PathBuf::from(expanded)
        } else {
            self.home.join(".ssh").join(expanded)
        };
        let s = full.display().to_string();
        match glob::glob(&s) {
            Ok(paths) => {
                let mut v: Vec<PathBuf> = paths.filter_map(Result::ok).filter(|p| p.is_file()).collect();
                v.sort();
                v
            }
            Err(_) => vec![full],
        }
    }
}

fn expand_home(s: &str, home: &Path) -> String {
    if s == "~" {
        return home.display().to_string();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return home.join(rest).display().to_string();
    }
    s.to_string()
}

fn is_pattern(p: &str) -> bool {
    p.contains('*') || p.contains('?') || p.starts_with('!')
}

/// Совпадение по шаблону OpenSSH: `*`, `?`.
pub fn wildcard_match(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let (mut pi, mut si, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn block_matches(patterns: &[String], alias: &str) -> bool {
    let mut positive = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if wildcard_match(neg, alias) {
                return false;
            }
        } else if wildcard_match(p, alias) {
            positive = true;
        }
    }
    positive
}

const SUPPORTED: &[&str] = &["host", "hostname", "port", "user", "identityfile", "match"];

pub fn parse(path: &Path) -> CoreResult<Parsed> {
    let home = dirs::home_dir().unwrap_or_default();
    parse_with_home(path, &home)
}

pub fn parse_with_home(path: &Path, home: &Path) -> CoreResult<Parsed> {
    let mut r = Reader {
        home: home.to_path_buf(),
        files: vec![],
        hasher: Sha256::new(),
        lines: vec![],
        warnings: vec![],
    };
    r.read(path, 0, &mut Vec::new(), true)?;

    // Строки → блоки.
    let mut blocks = vec![Block {
        patterns: None,
        header: String::new(),
        file: path.to_path_buf(),
        line: 0,
        is_match: false,
        opts: vec![],
    }];
    let mut ignored = Vec::new();
    for l in r.lines {
        match l.keyword.as_str() {
            "host" => blocks.push(Block {
                header: format!("Host {}", l.args.join(" ")),
                patterns: Some(l.args.clone()),
                file: l.file.clone(),
                line: l.line,
                is_match: false,
                opts: vec![],
            }),
            "match" => {
                r.warnings.push(
                    CoreError::new("import.match_unsupported", "Match blocks are not supported")
                        .with("file", l.file.display().to_string())
                        .with("line", l.line),
                );
                blocks.push(Block {
                    header: "Match".into(),
                    patterns: Some(vec![]),
                    file: l.file.clone(),
                    line: l.line,
                    is_match: true,
                    opts: vec![],
                });
            }
            k if SUPPORTED.contains(&k) => blocks.last_mut().expect("block").opts.push(l),
            _ => ignored.push(json!({
                "directive": l.keyword,
                "file": l.file.display().to_string(),
                "line": l.line,
            })),
        }
    }

    // Кандидаты — конкретные алиасы из Host.
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for b in blocks.iter().filter(|b| !b.is_match) {
        let Some(pats) = &b.patterns else { continue };
        for alias in pats.iter().filter(|p| !is_pattern(p)) {
            let dup = !seen.insert(alias.to_lowercase());
            candidates.push(resolve(alias, &blocks, b, dup, home));
        }
    }

    Ok(Parsed {
        source: path.to_path_buf(),
        files: r.files,
        candidates,
        ignored,
        warnings: r.warnings,
        digest: hex::encode(&r.hasher.finalize()[..16]),
    })
}

fn resolve(alias: &str, blocks: &[Block], origin: &Block, dup: bool, home: &Path) -> Candidate {
    let (mut hostname, mut port, mut user) = (None::<String>, None::<String>, None::<String>);
    let mut idents: Vec<String> = Vec::new();
    let mut defaults_from = Vec::new();
    let mut warnings = Vec::new();
    for b in blocks.iter().filter(|b| !b.is_match) {
        let applies = match &b.patterns {
            None => true,
            Some(p) => block_matches(p, alias),
        };
        if !applies {
            continue;
        }
        let mut contributed = false;
        for o in &b.opts {
            let Some(v) = o.args.first() else { continue };
            let slot = match o.keyword.as_str() {
                "hostname" => &mut hostname,
                "port" => &mut port,
                "user" => &mut user,
                "identityfile" => {
                    if !idents.contains(v) {
                        idents.push(v.clone());
                        contributed = true;
                    }
                    continue;
                }
                _ => continue,
            };
            if slot.is_none() {
                *slot = Some(v.clone());
                contributed = true;
            }
        }
        let is_template = b.patterns.as_ref().is_none_or(|p| p.iter().any(|x| is_pattern(x)));
        if contributed && is_template && !b.header.is_empty() {
            defaults_from.push(b.header.clone());
        }
    }
    let host = hostname
        .map(|h| h.replace("%h", alias).replace("%%", "%"))
        .unwrap_or_else(|| alias.to_string());
    let port_num = match port.as_deref().map(str::parse::<u16>) {
        None => 22,
        Some(Ok(p)) if p > 0 => p,
        Some(_) => {
            warnings.push(CoreError::invalid(
                "port",
                format!("invalid port {:?}", port.unwrap_or_default()),
            ));
            22
        }
    };
    if idents.len() > 1 {
        warnings.push(
            CoreError::new(
                "import.multiple_identity_files",
                "Several IdentityFile entries, the first one is used",
            )
            .with("key", alias)
            .with("used", idents[0].clone()),
        );
    }
    let local_user = crate::ssh::target::local_user();
    let remote_user = user.clone().unwrap_or_else(|| local_user.clone());
    let identity_file = idents.first().map(|f| {
        let f = expand_home(f, home);
        f.replace("%d", &home.display().to_string())
            .replace("%u", &local_user)
            .replace("%h", &host)
            .replace("%r", &remote_user)
            .replace("%%", "%")
    });
    Candidate {
        key: alias.to_string(),
        host,
        port: port_num,
        user,
        identity_file,
        file: origin.file.clone(),
        line: origin.line,
        defaults_from,
        warnings,
        duplicate_in_file: dup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard() {
        assert!(wildcard_match("web-*", "web-1"));
        assert!(wildcard_match("*", "x"));
        assert!(wildcard_match("db?", "db1"));
        assert!(!wildcard_match("db?", "db12"));
        assert!(block_matches(&["*".into(), "!bastion".into()], "web"));
        assert!(!block_matches(&["*".into(), "!bastion".into()], "bastion"));
    }

    #[test]
    fn parse_rules() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        std::fs::create_dir_all(ssh.join("config.d")).unwrap();
        std::fs::write(
            ssh.join("config"),
            "# comment\n\
             Include config.d/*\n\
             Host web-1 web-2\n  HostName 10.0.0.%h\n  User deploy\n\
             Host db-1\n  HostName=db.local\n  Port 2222\n  IdentityFile ~/.ssh/db_key\n  ProxyJump bastion\n\
             Host web-*\n  IdentityFile ~/.ssh/web_%r\n  Port 2200\n\
             Host *\n  User root\n  IdentityFile ~/.ssh/id_ed25519\n\
             Match host x\n  User nobody\n\
             Host web-1\n  HostName dup\n",
        )
        .unwrap();
        std::fs::write(
            ssh.join("config.d/work"),
            "Host work\n  HostName work.example\n  Include config\n",
        )
        .unwrap();
        let p = parse_with_home(&ssh.join("config"), home.path()).unwrap();
        let get = |k: &str| {
            p.candidates
                .iter()
                .find(|c| c.key == k && !c.duplicate_in_file)
                .unwrap()
        };

        let w = get("web-1");
        assert_eq!(w.host, "10.0.0.web-1");
        assert_eq!(w.port, 2200);
        assert_eq!(w.user.as_deref(), Some("deploy"));
        let web_key = home.path().join(".ssh/web_deploy").display().to_string();
        assert_eq!(w.identity_file.as_deref(), Some(web_key.as_str()));
        assert_eq!(w.defaults_from, vec!["Host web-*", "Host *"]);
        assert!(w.warnings.iter().any(|e| e.code == "import.multiple_identity_files"));

        let d = get("db-1");
        assert_eq!((d.host.as_str(), d.port), ("db.local", 2222));
        assert_eq!(d.user.as_deref(), Some("root"));

        assert_eq!(get("work").host, "work.example");
        assert!(p.candidates.iter().any(|c| c.key == "web-1" && c.duplicate_in_file));
        assert!(!p.candidates.iter().any(|c| c.key.contains('*')));
        assert_eq!(p.files.len(), 2);
        assert!(p.warnings.iter().any(|w| w.code == "import.include_cycle"));
        assert!(p.warnings.iter().any(|w| w.code == "import.match_unsupported"));
        assert_eq!(p.ignored[0]["directive"], "proxyjump");
        assert_eq!(p.digest.len(), 32);
    }

    #[test]
    fn missing_file() {
        let e = parse_with_home(Path::new("/nonexistent/cfg"), Path::new("/tmp")).unwrap_err();
        assert_eq!(e.code, "fs.not_found");
    }
}
