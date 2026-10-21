//! Требует sshd (SSH_TEST_HOST/PORT/KEY, см. scripts/integration.sh); без них пропускается.
use servergui_core::{
    known_hosts::KnownHosts,
    model::*,
    monitor::{MonitorEvent, Snapshot},
    secrets::{MemoryStore, SecretStore},
    ssh::SshPool,
    store::ServerStore,
    CoreError,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::UnboundedReceiver;

async fn next_snapshot(rx: &mut UnboundedReceiver<MonitorEvent>) -> Snapshot {
    loop {
        match tokio::time::timeout(Duration::from_secs(15), rx.recv()).await {
            Ok(Some(MonitorEvent::Snapshot(s))) => return *s,
            Ok(Some(MonitorEvent::Error { message })) => panic!("ошибка монитора: {message}"),
            Ok(Some(MonitorEvent::Closed { reason })) => panic!("монитор закрыт: {reason}"),
            Ok(None) => panic!("поток закрылся"),
            Err(_) => panic!("таймаут ожидания снимка"),
        }
    }
}

#[tokio::test]
async fn monitor_snapshots_and_kill() {
    let (Ok(host), Ok(port), Ok(key)) = (std::env::var("SSH_TEST_HOST"), std::env::var("SSH_TEST_PORT"), std::env::var("SSH_TEST_KEY")) else {
        eprintln!("SSH_TEST_* не заданы — пропуск");
        return;
    };
    let port: u16 = port.parse().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let secrets: Arc<MemoryStore> = Arc::new(MemoryStore::default());
    let store = ServerStore::new(dir.path().join("servers.json"), secrets.clone());
    let pool = SshPool::new(KnownHosts::new(dir.path().join("known_hosts")));
    let srv = store
        .create(ServerInput { name: "t".into(), host: host.clone(), port, username: "tester".into(), group: None, enhanced_shell: false,
            auth: AuthInput::Key { key_path: Some(key), key_pem: None, passphrase: None } })
        .unwrap();
    let s: &dyn SecretStore = secrets.as_ref();
    let fp = match pool.connect(&srv, s).await {
        Err(CoreError::UnknownHost { fingerprint, .. }) => fingerprint,
        other => panic!("{other:?}"),
    };
    pool.trust_host(&host, port, &fp).unwrap();

    // sleep-процесс с уникальным аргументом
    let r = pool.exec(&srv, s, "nohup sleep 7777 >/dev/null 2>&1 & echo $!").await.unwrap();
    let sleep_pid: u32 = r.stdout.trim().parse().expect("pid sleep");

    let (mid, mut rx) = pool.monitor_start(&srv, s, 1000).await.unwrap();
    let first = next_snapshot(&mut rx).await;
    assert!(!first.rates_valid);
    let snap = next_snapshot(&mut rx).await;
    assert!(snap.rates_valid);
    let third = next_snapshot(&mut rx).await;
    assert!(third.uptime_s > first.uptime_s);

    // разумность значений
    assert!(!snap.info.hostname.is_empty(), "hostname");
    assert!(!snap.info.os.is_empty(), "os");
    assert!(!snap.info.kernel.is_empty(), "kernel");
    assert!(snap.info.cores >= 1 && snap.cpu.cores.len() == snap.info.cores);
    assert!((0.0..=100.0).contains(&snap.cpu.total));
    assert!(snap.cpu.cores.iter().all(|c| (0.0..=100.0).contains(c)));
    assert!(snap.mem.total > 0 && snap.mem.used <= snap.mem.total && snap.mem.available <= snap.mem.total);
    assert!(snap.swap_ok());
    assert!(snap.uptime_s > 0.0);
    assert!(snap.load.iter().all(|l| *l >= 0.0));
    assert!(snap.net_rx_bps >= 0.0 && snap.disk_read_bps >= 0.0);
    assert!(!snap.filesystems.is_empty(), "df");
    assert!(snap.filesystems.iter().all(|f| (0.0..=100.0).contains(&f.percent) && f.used <= f.total));
    assert!(snap.processes.len() >= 2);
    let sleeper = snap.processes.iter().find(|p| p.pid == sleep_pid).expect("sleep в списке процессов");
    assert!(sleeper.args.contains("sleep 7777"), "{sleeper:?}");
    assert_eq!(sleeper.user, "tester");

    // ошибки kill текстом
    assert!(pool.process_kill(&srv, s, sleep_pid, "BOGUS").await.is_err());
    assert!(pool.process_kill(&srv, s, 1, "KILL").await.is_err());
    let root_pid = snap.processes.iter().find(|p| p.user == "root" && p.pid > 1).map(|p| p.pid);
    if let Some(pid) = root_pid {
        let e = pool.process_kill(&srv, s, pid, "KILL").await.expect_err("чужой процесс убить нельзя");
        eprintln!("kill root-процесса: {e}");
        assert!(!e.to_string().is_empty());
    }

    // убиваем sleep и ждём исчезновения из списка
    pool.process_kill(&srv, s, sleep_pid, "TERM").await.unwrap();
    let mut gone = false;
    for _ in 0..6 {
        if !next_snapshot(&mut rx).await.processes.iter().any(|p| p.pid == sleep_pid) {
            gone = true;
            break;
        }
    }
    assert!(gone, "sleep не исчез после TERM");
    assert!(pool.process_kill(&srv, s, sleep_pid, "TERM").await.is_err(), "повторный kill несуществующего pid");

    // stop -> Closed
    pool.monitor_stop(&mid);
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("таймаут Closed") {
            Some(MonitorEvent::Closed { .. }) => break,
            Some(_) => {}
            None => panic!("нет Closed"),
        }
    }

    // disconnect останавливает мониторы
    let (_m2, mut rx2) = pool.monitor_start(&srv, s, 1000).await.unwrap();
    next_snapshot(&mut rx2).await;
    pool.disconnect(&srv.id).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx2.recv()).await.expect("монитор не остановился при disconnect") {
            Some(MonitorEvent::Closed { .. }) => break,
            Some(_) => {}
            None => panic!("нет Closed"),
        }
    }
}

trait SwapOk {
    fn swap_ok(&self) -> bool;
}
impl SwapOk for Snapshot {
    fn swap_ok(&self) -> bool {
        self.mem.swap_used <= self.mem.swap_total
    }
}
