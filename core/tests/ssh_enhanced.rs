//! «Улучшенная оболочка» против настоящего sshd. Требует SSH_TEST_HOST/PORT/KEY и SSH_TEST_EXPECT=zsh|bash
//! (какая оболочка должна выбраться на этом сервере). Без переменных пропускается.
use sherpa_core::{
    known_hosts::KnownHosts,
    model::*,
    secrets::{MemoryStore, SecretStore},
    ssh::SshPool,
    store::ServerStore,
    terminal::TermEvent,
    CoreError,
};
use std::{sync::Arc, time::{Duration, Instant}};
use tokio::sync::mpsc::UnboundedReceiver;

async fn read_until(rx: &mut UnboundedReceiver<TermEvent>, needle: &str, acc: &mut String) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !acc.contains(needle) {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some(TermEvent::Data { data })) => acc.push_str(&String::from_utf8_lossy(&data)),
            Ok(_) => panic!("терминал закрыт, вывод: {acc:?}"),
            Err(_) => panic!("таймаут ожидания {needle:?}, вывод: {acc:?}"),
        }
    }
}

/// Снимок домашнего каталога (имена и md5 файлов) без ~/.sherpa.
/// ./logs пропускаем: это журнал самого sshd в образе linuxserver (пишется при каждом входе), а не следы оболочки.
const SNAPSHOT: &str = r#"cd "$HOME" && { find . \( -path ./.sherpa -o -path ./logs \) -prune -o -print | sort; find . \( -path ./.sherpa -o -path ./logs \) -prune -o -type f -exec md5sum {} \; | sort; }"#;

#[tokio::test]
async fn enhanced_shell_roundtrip() {
    let (Ok(host), Ok(port), Ok(key), Ok(expect)) =
        (std::env::var("SSH_TEST_HOST"), std::env::var("SSH_TEST_PORT"), std::env::var("SSH_TEST_KEY"), std::env::var("SSH_TEST_EXPECT"))
    else {
        eprintln!("SSH_TEST_* не заданы — пропуск");
        return;
    };
    let port: u16 = port.parse().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let secrets: Arc<MemoryStore> = Arc::new(MemoryStore::default());
    let s: &dyn SecretStore = secrets.as_ref();
    let store = ServerStore::new(dir.path().join("servers.json"), secrets.clone());
    let pool = SshPool::new(KnownHosts::new(dir.path().join("known_hosts")));
    let srv = store
        .create(ServerInput { name: "t".into(), host: host.clone(), port, username: "tester".into(), group: None, enhanced_shell: false,
            auth: AuthInput::Key { key_path: Some(key), key_pem: None, passphrase: None } })
        .unwrap();
    let CoreError::UnknownHost { fingerprint, .. } = pool.connect(&srv, s).await.unwrap_err() else { panic!() };
    pool.trust_host(&host, port, &fingerprint).unwrap();

    let before = pool.exec(&srv, s, SNAPSHOT).await.unwrap().stdout;
    assert!(!before.is_empty());

    // установка: первый раз заливает, второй — версия совпала, ничего не шлём
    let st = pool.shell_install(&srv, s).await.unwrap();
    assert!(st.uploaded);
    assert_eq!(st.shell, expect);
    assert_eq!(st.install_hint.is_some(), expect != "zsh", "{st:?}");
    if let Some(h) = &st.install_hint { assert!(h.contains("zsh"), "{h}"); }
    assert!(!pool.shell_install(&srv, s).await.unwrap().uploaded);
    // порча версии → перезаливка
    pool.exec(&srv, s, "echo old > $HOME/.sherpa/shell/.version").await.unwrap();
    assert!(pool.shell_install(&srv, s).await.unwrap().uploaded);
    assert_eq!(pool.exec(&srv, s, SNAPSHOT).await.unwrap().stdout, before, "изменилось что-то вне ~/.sherpa");

    // терминал с включённой опцией
    let srv = store.set_enhanced_shell(&srv.id, true).unwrap();
    let (info, mut rx) = pool.terminal_open(&srv, s, 100, 30).await.unwrap();
    assert_eq!(info.shell, expect);
    let mut out = String::new();
    read_until(&mut rx, "㉿", &mut out).await;
    assert!(out.contains("┌──(") && out.contains("tester㉿"), "{out:?}");
    if expect == "zsh" {
        pool.terminal_write(&info.terminal_id, b"whence -w _zsh_autosuggest_start; echo HL=$ZSH_HIGHLIGHT_VERSION\n".to_vec()).unwrap();
        read_until(&mut rx, "HL=0.8.0", &mut out).await;
        assert!(out.contains("_zsh_autosuggest_start: function"), "{out:?}");
    }
    pool.terminal_write(&info.terminal_id, b"echo marker-$((6*7)); exit\n".to_vec()).unwrap();
    read_until(&mut rx, "marker-42", &mut out).await;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("таймаут") {
            Some(TermEvent::Closed { exit_code, .. }) => { assert_eq!(exit_code, Some(0)); break; }
            Some(_) => {}
            None => panic!("нет Closed"),
        }
    }
    // после сессии (история и т.п.) вне ~/.sherpa ничего не появилось
    assert_eq!(pool.exec(&srv, s, SNAPSHOT).await.unwrap().stdout, before, "сессия оставила следы в ~");

    // удаление с сервера
    pool.shell_uninstall(&srv, s).await.unwrap();
    assert_eq!(pool.exec(&srv, s, "test -e $HOME/.sherpa && echo yes || echo no").await.unwrap().stdout.trim(), "no");
    assert_eq!(pool.exec(&srv, s, SNAPSHOT).await.unwrap().stdout, before);
}
