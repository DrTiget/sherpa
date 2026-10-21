//! «Улучшенная оболочка»: набор файлов (zshrc/bashrc в стиле Kali + плагины zsh), которые заливаются
//! в ~/.sherpa/shell на сервере. Больше ничего на сервере не трогаем.
use include_dir::{include_dir, Dir};
use serde::Serialize;
use sha2::{Digest, Sha256};

static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/shell");

pub const REMOTE_DIR: &str = "$HOME/.sherpa/shell";

/// Хэш содержимого всех ресурсов — версия набора на сервере.
pub fn version() -> String {
    let mut files = Vec::new();
    collect(&ASSETS, &mut files);
    files.sort_by_key(|(p, _)| p.clone());
    let mut h = Sha256::new();
    for (p, c) in files {
        h.update(p.as_bytes());
        h.update([0]);
        h.update(&c);
    }
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn collect(d: &Dir<'_>, out: &mut Vec<(String, Vec<u8>)>) {
    for f in d.files() {
        out.push((f.path().to_string_lossy().replace('\\', "/"), f.contents().to_vec()));
    }
    for s in d.dirs() {
        collect(s, out);
    }
}

/// tar-архив с ресурсами и файлом `.version` (кладётся последним).
pub fn build_tar() -> Vec<u8> {
    let mut files = Vec::new();
    collect(&ASSETS, &mut files);
    files.sort_by_key(|(p, _)| p.clone());
    let ver = version();
    files.push((".version".into(), ver.into_bytes()));
    let mut b = tar::Builder::new(Vec::new());
    for (path, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(0);
        h.set_uid(0);
        h.set_gid(0);
        b.append_data(&mut h, &path, data.as_slice()).expect("tar в памяти");
    }
    b.into_inner().expect("tar в памяти")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ShellProbe {
    /// Версия набора, уже лежащего на сервере.
    pub remote_version: Option<String>,
    pub has_zsh: bool,
    pub has_bash: bool,
    pub os_id: String,
}

/// Одна команда-разведка на сервере. Формат вывода: version / zsh|nozsh / bash|nobash / "<ID> <ID_LIKE>".
pub const PROBE_CMD: &str = "cat \"$HOME/.sherpa/shell/.version\" 2>/dev/null; echo; \
command -v zsh >/dev/null 2>&1 && echo zsh || echo nozsh; \
command -v bash >/dev/null 2>&1 && echo bash || echo nobash; \
( . /etc/os-release 2>/dev/null; echo \"$ID $ID_LIKE\" )";

pub fn parse_probe(out: &str) -> ShellProbe {
    let mut l = out.lines();
    let v = l.next().unwrap_or("").trim();
    ShellProbe {
        remote_version: (!v.is_empty()).then(|| v.to_string()),
        has_zsh: l.next().map(str::trim) == Some("zsh"),
        has_bash: l.next().map(str::trim) == Some("bash"),
        os_id: l.next().unwrap_or("").trim().to_lowercase(),
    }
}

/// Готовая команда установки zsh под дистрибутив.
pub fn zsh_install_hint(os_id: &str, is_root: bool) -> String {
    let sudo = if is_root { "" } else { "sudo " };
    let has = |names: &[&str]| os_id.split_whitespace().any(|w| names.contains(&w));
    if has(&["debian", "ubuntu", "kali", "linuxmint", "raspbian"]) {
        format!("{sudo}apt install -y zsh")
    } else if has(&["fedora", "rhel", "centos", "rocky", "almalinux", "ol", "amzn"]) {
        format!("{sudo}dnf install -y zsh")
    } else if has(&["alpine"]) {
        format!("{sudo}apk add zsh")
    } else if has(&["arch", "manjaro"]) {
        format!("{sudo}pacman -S --noconfirm zsh")
    } else if has(&["opensuse", "suse", "sles"]) {
        format!("{sudo}zypper install -y zsh")
    } else {
        "установите пакет zsh средствами вашего дистрибутива".into()
    }
}

/// Команда запуска интерактивной оболочки.
pub fn launch_cmd(probe: &ShellProbe) -> Option<(&'static str, String)> {
    if probe.has_zsh {
        Some(("zsh", format!("ZDOTDIR=\"{REMOTE_DIR}\" exec zsh -i")))
    } else if probe.has_bash {
        Some(("bash", format!("exec bash --rcfile \"{REMOTE_DIR}/bashrc\" -i")))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_stable_and_tar_contains_files() {
        assert_eq!(version(), version());
        let data = build_tar();
        let mut a = tar::Archive::new(data.as_slice());
        let names: Vec<String> = a.entries().unwrap().map(|e| e.unwrap().path().unwrap().to_string_lossy().into()).collect();
        for want in [".zshrc", "bashrc", ".version", "plugins/zsh-autosuggestions/zsh-autosuggestions.zsh"] {
            assert!(names.iter().any(|n| n == want), "нет {want} в {names:?}");
        }
    }

    #[test]
    fn probe_and_hints() {
        let p = parse_probe("abc123\nzsh\nbash\nubuntu debian\n");
        assert_eq!(p, ShellProbe { remote_version: Some("abc123".into()), has_zsh: true, has_bash: true, os_id: "ubuntu debian".into() });
        let p = parse_probe("\nnozsh\nbash\nalpine \n");
        assert_eq!(p.remote_version, None);
        assert_eq!(launch_cmd(&p).unwrap().0, "bash");
        assert_eq!(zsh_install_hint(&p.os_id, true), "apk add zsh");
        assert_eq!(zsh_install_hint("rocky rhel centos fedora", false), "sudo dnf install -y zsh");
        assert_eq!(zsh_install_hint("kali debian", false), "sudo apt install -y zsh");
        assert!(launch_cmd(&parse_probe("\nnozsh\nnobash\n")).is_none());
    }
}
