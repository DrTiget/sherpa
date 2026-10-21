//! Монитор ресурсов: один долгоживущий exec-канал со shell-скриптом-циклом, без установки агента на сервер.
//! Скрипт раз в N секунд печатает блоки `@@SG:<имя>` из /proc, `df` и `ps`; здесь они разбираются в [`Snapshot`].
//! Только Linux: на остальных ОС скрипт сообщает `@@SG:UNSUPPORTED` и завершается.
use crate::{CoreError, CoreResult};
use russh::{client::Msg, Channel, ChannelMsg};
use serde::Serialize;
use std::{collections::HashMap, sync::{Arc, Mutex}};
use tokio::sync::mpsc;

pub const MIN_INTERVAL_MS: u64 = 500;
pub const MAX_INTERVAL_MS: u64 = 60_000;
pub const DEFAULT_INTERVAL_MS: u64 = 2_000;
const MARK: &str = "@@SG:";

/// Допустимые сигналы для `process_kill`.
pub const SIGNALS: &[&str] = &["TERM", "KILL", "HUP", "INT", "STOP", "CONT"];

/// Скрипт без одинарных кавычек (оборачивается в `sh -c '…'`). Плейсхолдеры: `__SLEEP__`, `__SLEEP_INT__`.
const SCRIPT: &str = r#"[ "$(uname -s 2>/dev/null)" = Linux ] || { echo "@@SG:UNSUPPORTED $(uname -s 2>/dev/null)"; exit 3; }
first=1
while :; do
echo "@@SG:BEGIN"
if [ $first = 1 ]; then
first=0
echo "@@SG:host"; hostname 2>/dev/null || cat /proc/sys/kernel/hostname
echo "@@SG:osrel"; cat /etc/os-release 2>/dev/null || cat /usr/lib/os-release 2>/dev/null
echo "@@SG:kernel"; uname -r
fi
for f in stat meminfo loadavg uptime net/dev diskstats; do echo "@@SG:$f"; cat /proc/$f 2>/dev/null; done
echo "@@SG:df"; df -P -k 2>/dev/null
echo "@@SG:ps"
out=$(ps -eo pid,ppid,user,pcpu,pmem,rss,etime,stat,comm,args --sort=-pcpu 2>/dev/null | head -n 201)
if [ -n "$out" ]; then echo full; echo "$out"; else echo min; ps -eo pid,ppid,user,rss,etime,stat,comm,args 2>/dev/null | head -n 201; fi
echo "@@SG:END"
sleep __SLEEP__ 2>/dev/null || sleep __SLEEP_INT__
done"#;

/// Команда для exec-канала.
pub fn command(interval_ms: u64) -> String {
    let ms = interval_ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
    let secs = format!("{}.{:03}", ms / 1000, ms % 1000);
    let script = SCRIPT.replace("__SLEEP__", &secs).replace("__SLEEP_INT__", &ms.div_ceil(1000).to_string());
    format!("exec sh -c '{script}'")
}

// ---------- модель ----------

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct HostInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub cores: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Cpu {
    /// Общая загрузка, %.
    pub total: f64,
    pub cores: Vec<f64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Mem {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct NetIf {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DiskDev {
    pub name: String,
    pub read_bps: f64,
    pub write_bps: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Filesystem {
    pub device: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub cpu: f64,
    pub mem: f64,
    pub rss: u64,
    pub etime: String,
    pub state: String,
    pub comm: String,
    pub args: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Snapshot {
    /// Время получения на клиенте, мс с эпохи.
    pub ts_ms: u64,
    /// false у первого снимка: скорости и CPU считаются по разности и ещё равны нулю.
    pub rates_valid: bool,
    pub info: HostInfo,
    pub uptime_s: f64,
    pub load: [f64; 3],
    pub cpu: Cpu,
    pub mem: Mem,
    pub net: Vec<NetIf>,
    pub net_rx_bps: f64,
    pub net_tx_bps: f64,
    pub disks: Vec<DiskDev>,
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub filesystems: Vec<Filesystem>,
    pub processes: Vec<Process>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MonitorEvent {
    Snapshot(Box<Snapshot>),
    /// Нефатальная/фатальная ошибка текстом (например, «не поддерживается»).
    Error { message: String },
    Closed { reason: String },
}

// ---------- парсеры ----------

/// (всего, простой) в «тиках» для строки cpu.
pub type CpuTimes = (u64, u64);

pub fn parse_stat(s: &str) -> (Option<CpuTimes>, Vec<CpuTimes>) {
    let (mut total, mut cores) = (None, Vec::new());
    for line in s.lines() {
        let mut it = line.split_whitespace();
        let Some(name) = it.next() else { continue };
        if !name.starts_with("cpu") {
            continue;
        }
        let v: Vec<u64> = it.map_while(|x| x.parse().ok()).collect();
        if v.len() < 4 {
            continue;
        }
        let sum: u64 = v.iter().take(8).sum(); // guest уже входит в user/nice
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        if name == "cpu" { total = Some((sum, idle)) } else { cores.push((sum, idle)) }
    }
    (total, cores)
}

fn cpu_pct(prev: CpuTimes, cur: CpuTimes) -> f64 {
    let dt = cur.0.saturating_sub(prev.0);
    if dt == 0 {
        return 0.0;
    }
    let di = cur.1.saturating_sub(prev.1).min(dt);
    ((dt - di) as f64 / dt as f64 * 100.0).clamp(0.0, 100.0)
}

pub fn parse_meminfo(s: &str) -> Mem {
    let mut m: HashMap<&str, u64> = HashMap::new();
    for line in s.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if let Some(n) = v.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()) {
                m.insert(k.trim(), n * 1024);
            }
        }
    }
    let g = |k: &str| m.get(k).copied().unwrap_or(0);
    let total = g("MemTotal");
    let available = m.get("MemAvailable").copied().unwrap_or_else(|| g("MemFree") + g("Buffers") + g("Cached")).min(total);
    Mem {
        total,
        used: total - available,
        available,
        buffers: g("Buffers"),
        cached: g("Cached") + g("SReclaimable"),
        swap_total: g("SwapTotal"),
        swap_used: g("SwapTotal").saturating_sub(g("SwapFree")),
    }
}

pub fn parse_loadavg(s: &str) -> [f64; 3] {
    let mut it = s.split_whitespace().map(|x| x.parse::<f64>().unwrap_or(0.0));
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

pub fn parse_uptime(s: &str) -> f64 {
    s.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0.0)
}

/// iface -> (rx_bytes, tx_bytes); loopback пропускается.
pub fn parse_net_dev(s: &str) -> Vec<(String, u64, u64)> {
    s.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let name = name.trim();
            if name == "lo" {
                return None;
            }
            let v: Vec<u64> = rest.split_whitespace().map_while(|x| x.parse().ok()).collect();
            (v.len() >= 9).then(|| (name.to_string(), v[0], v[8]))
        })
        .collect()
}

/// Физические устройства -> (прочитано байт, записано байт). Разделы, loop/ram/dm отбрасываются, чтобы не считать дважды.
pub fn parse_diskstats(s: &str) -> Vec<(String, u64, u64)> {
    let all: Vec<(String, u64, u64)> = s
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 10 {
                return None;
            }
            Some((f[2].to_string(), f[5].parse::<u64>().ok()? * 512, f[9].parse::<u64>().ok()? * 512))
        })
        .collect();
    let names: Vec<&str> = all.iter().map(|d| d.0.as_str()).collect();
    let is_partition = |n: &str| {
        names.iter().any(|p| {
            n.len() > p.len()
                && n.starts_with(p)
                && p.chars().last().is_some_and(|c| !c.is_ascii_digit() || n[p.len()..].starts_with('p'))
                && n[p.len()..].trim_start_matches('p').chars().all(|c| c.is_ascii_digit())
        })
    };
    all.iter()
        .filter(|(n, ..)| !["loop", "ram", "dm-", "zram", "sr", "fd"].iter().any(|p| n.starts_with(p)) && !is_partition(n))
        .cloned()
        .collect()
}

pub fn parse_df(s: &str) -> Vec<Filesystem> {
    s.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 6 {
                return None;
            }
            let (total, used, avail): (u64, u64, u64) = (f[1].parse::<u64>().ok()? * 1024, f[2].parse::<u64>().ok()? * 1024, f[3].parse::<u64>().ok()? * 1024);
            let mount = f[5..].join(" ");
            let skip_dev = ["devtmpfs", "udev", "squashfs", "efivarfs", "shm", "none", "overlay_lower"];
            let skip_mnt = ["/dev", "/sys", "/proc", "/run/user", "/run/lock", "/run/snapd", "/snap", "/var/lib/docker"];
            if total == 0
                || skip_dev.contains(&f[0])
                || skip_mnt.iter().any(|p| mount == *p || mount.starts_with(&format!("{p}/")))
            {
                return None;
            }
            // как в df: занято / (занято + доступно), учитывает резерв root
            let denom = (used + avail) as f64;
            Some(Filesystem {
                device: f[0].into(),
                mount,
                total,
                used,
                available: avail,
                percent: if denom > 0.0 { used as f64 / denom * 100.0 } else { 0.0 },
            })
        })
        .collect()
}

pub fn parse_os_release(s: &str) -> String {
    let get = |key: &str| {
        s.lines().find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix('=')).map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string()))
    };
    get("PRETTY_NAME")
        .or_else(|| get("NAME").map(|n| format!("{n} {}", get("VERSION_ID").unwrap_or_default()).trim().to_string()))
        .unwrap_or_default()
}

/// `variant`: "full" — pid,ppid,user,pcpu,pmem,rss,etime,stat,comm,args; "min" — pid,ppid,user,rss,etime,stat,comm,args (busybox).
/// Первая строка блока — заголовок ps, пропускается. args — всё остальное в строке.
pub fn parse_ps(variant: &str, s: &str) -> Vec<Process> {
    let full = variant == "full";
    let fixed = if full { 9 } else { 7 };
    s.lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let f: Vec<&str> = it.by_ref().take(fixed).collect();
            if f.len() < fixed {
                return None;
            }
            let args = l.trim_start();
            // args — хвост строки после fixed токенов
            let mut rest = args;
            for _ in 0..fixed {
                rest = rest.trim_start();
                rest = &rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..];
            }
            let rest = rest.trim().to_string();
            let o = if full { 2 } else { 0 };
            let p = Process {
                pid: f[0].parse().ok()?,
                ppid: f[1].parse().unwrap_or(0),
                user: f[2].to_string(),
                cpu: if full { f[3].parse().unwrap_or(0.0) } else { 0.0 },
                mem: if full { f[4].parse().unwrap_or(0.0) } else { 0.0 },
                rss: f[3 + o].parse::<u64>().unwrap_or(0) * 1024,
                etime: f[4 + o].to_string(),
                state: f[5 + o].to_string(),
                comm: f[6 + o].to_string(),
                args: if rest.is_empty() { f[6 + o].to_string() } else { rest },
            };
            Some(p)
        })
        .collect()
}

// ---------- сборка снимков ----------

#[derive(Default)]
struct Raw {
    uptime: f64,
    cpu: Option<CpuTimes>,
    cores: Vec<CpuTimes>,
    net: HashMap<String, (u64, u64)>,
    disks: HashMap<String, (u64, u64)>,
}

/// Потоковый разборщик вывода скрипта: подаётся построчно, по `@@SG:END` выдаёт снимок.
#[derive(Default)]
pub struct Collector {
    info: HostInfo,
    prev: Option<Raw>,
    section: String,
    ps_variant: String,
    bufs: HashMap<String, String>,
    pub unsupported: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Collector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_line(&mut self, line: &str) -> Option<Snapshot> {
        let line = line.trim_end_matches(['\r', '\n']);
        if let Some(tag) = line.strip_prefix(MARK) {
            match tag {
                "BEGIN" => self.bufs.clear(),
                "END" => return self.finish(),
                t if t.starts_with("UNSUPPORTED") => self.unsupported = Some(t.trim_start_matches("UNSUPPORTED").trim().to_string()),
                t => self.section = t.to_string(),
            }
            return None;
        }
        if self.section == "ps" && self.ps_variant.is_empty() {
            self.ps_variant = line.trim().to_string(); // full | min
            return None;
        }
        let b = self.bufs.entry(self.section.clone()).or_default();
        b.push_str(line);
        b.push('\n');
        None
    }

    fn finish(&mut self) -> Option<Snapshot> {
        let get = |bufs: &HashMap<String, String>, k: &str| bufs.get(k).cloned().unwrap_or_default();
        let b = std::mem::take(&mut self.bufs);
        let variant = std::mem::take(&mut self.ps_variant);
        if let Some(h) = b.get("host") {
            self.info.hostname = h.trim().to_string();
        }
        if let Some(o) = b.get("osrel") {
            self.info.os = parse_os_release(o);
        }
        if let Some(k) = b.get("kernel") {
            self.info.kernel = k.trim().to_string();
        }
        let (cpu_all, cores) = parse_stat(&get(&b, "stat"));
        if !cores.is_empty() {
            self.info.cores = cores.len();
        }
        let uptime = parse_uptime(&get(&b, "uptime"));
        let net = parse_net_dev(&get(&b, "net/dev"));
        let disks = parse_diskstats(&get(&b, "diskstats"));

        let mut snap = Snapshot {
            ts_ms: now_ms(),
            info: self.info.clone(),
            uptime_s: uptime,
            load: parse_loadavg(&get(&b, "loadavg")),
            mem: parse_meminfo(&get(&b, "meminfo")),
            filesystems: parse_df(&get(&b, "df")),
            processes: parse_ps(&variant, &get(&b, "ps")),
            cpu: Cpu { total: 0.0, cores: vec![0.0; cores.len()] },
            net: net.iter().map(|(n, rx, tx)| NetIf { name: n.clone(), rx_total: *rx, tx_total: *tx, ..Default::default() }).collect(),
            disks: disks.iter().map(|(n, ..)| DiskDev { name: n.clone(), ..Default::default() }).collect(),
            ..Default::default()
        };
        if let Some(p) = &self.prev {
            let dt = uptime - p.uptime;
            if dt > 0.05 {
                snap.rates_valid = true;
                if let (Some(a), Some(c)) = (p.cpu, cpu_all) {
                    snap.cpu.total = cpu_pct(a, c);
                }
                for (i, c) in cores.iter().enumerate() {
                    if let Some(a) = p.cores.get(i) {
                        snap.cpu.cores[i] = cpu_pct(*a, *c);
                    }
                }
                let rate = |new: u64, old: u64| new.saturating_sub(old) as f64 / dt;
                for n in &mut snap.net {
                    if let Some(&(rx0, tx0)) = p.net.get(&n.name) {
                        n.rx_bps = rate(n.rx_total, rx0);
                        n.tx_bps = rate(n.tx_total, tx0);
                    }
                }
                for (d, (name, r, w)) in snap.disks.iter_mut().zip(&disks) {
                    if let Some(&(r0, w0)) = p.disks.get(name) {
                        d.read_bps = rate(*r, r0);
                        d.write_bps = rate(*w, w0);
                    }
                }
            }
        }
        snap.net_rx_bps = snap.net.iter().map(|n| n.rx_bps).sum();
        snap.net_tx_bps = snap.net.iter().map(|n| n.tx_bps).sum();
        snap.disk_read_bps = snap.disks.iter().map(|d| d.read_bps).sum();
        snap.disk_write_bps = snap.disks.iter().map(|d| d.write_bps).sum();

        self.prev = Some(Raw {
            uptime,
            cpu: cpu_all,
            cores,
            net: net.into_iter().map(|(n, rx, tx)| (n, (rx, tx))).collect(),
            disks: disks.into_iter().map(|(n, r, w)| (n, (r, w))).collect(),
        });
        Some(snap)
    }
}

// ---------- задача и хэндл ----------

enum Cmd {
    Close,
}

#[derive(Clone)]
pub struct MonitorHandle {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl MonitorHandle {
    pub fn close(&self) {
        let _ = self.tx.send(Cmd::Close);
    }
}

/// monitor_id -> (server_id, handle)
pub type Registry = Arc<Mutex<HashMap<String, (String, MonitorHandle)>>>;

pub fn spawn(mut ch: Channel<Msg>, monitor_id: String, events: mpsc::UnboundedSender<MonitorEvent>, registry: Registry) -> MonitorHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut col = Collector::new();
        let (mut pending, mut stderr) = (Vec::<u8>::new(), String::new());
        let mut reason = "соединение закрыто".to_string();
        let mut exit_code = None;
        'main: loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let _ = cmd;
                    reason = "остановлен".into();
                    let _ = ch.close().await;
                    break;
                }
                msg = ch.wait() => match msg {
                    Some(ChannelMsg::Data { data }) => {
                        pending.extend_from_slice(&data);
                        while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = pending.drain(..=pos).collect();
                            if let Some(snap) = col.push_line(&String::from_utf8_lossy(&line)) {
                                if events.send(MonitorEvent::Snapshot(Box::new(snap))).is_err() { let _ = ch.close().await; break 'main; }
                            }
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if stderr.len() < 2000 { stderr.push_str(&String::from_utf8_lossy(&data)); }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
                    Some(ChannelMsg::Close) | None => break,
                    _ => {}
                },
            }
        }
        registry.lock().unwrap().remove(&monitor_id);
        if let Some(os) = col.unsupported.take() {
            let _ = events.send(MonitorEvent::Error { message: format!("Монитор ресурсов не поддерживается: сервер работает на {} (нужен Linux)", if os.is_empty() { "неизвестной ОС" } else { &os }) });
        } else if exit_code.is_some_and(|c| c != 0) && !stderr.trim().is_empty() {
            let _ = events.send(MonitorEvent::Error { message: format!("Сбор метрик завершился с ошибкой: {}", stderr.trim()) });
        }
        let _ = events.send(MonitorEvent::Closed { reason });
    });
    MonitorHandle { tx }
}

/// Проверка аргументов `process_kill`; возвращает команду для exec.
pub fn kill_command(pid: u32, signal: &str) -> CoreResult<String> {
    let sig = signal.trim_start_matches("SIG").to_ascii_uppercase();
    if !SIGNALS.contains(&sig.as_str()) {
        return Err(CoreError::msg(format!("Неподдерживаемый сигнал: {signal}")));
    }
    if pid <= 1 {
        return Err(CoreError::msg("Нельзя посылать сигнал процессу с PID ≤ 1"));
    }
    Ok(format!("kill -s {sig} -- {pid} 2>&1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT1: &str = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 50 0 50 350 50 0 0 0 0 0\ncpu1 50 0 50 350 50 0 0 0 0 0\nintr 1 2 3\nctxt 5\n";
    const STAT2: &str = "cpu  200 0 200 750 150 0 0 0 0 0\ncpu0 150 0 100 350 50 0 0 0 0 0\ncpu1 50 0 100 400 100 0 0 0 0 0\n";
    const MEMINFO: &str = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    8000000 kB\nBuffers:          500000 kB\nCached:          4000000 kB\nSReclaimable:     100000 kB\nSwapTotal:       2000000 kB\nSwapFree:        1500000 kB\n";
    const NETDEV: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n  eth0:5000000 400 0 0 0 0 0 0 250000 300 0 0 0 0 0 0\n";
    const DISKSTATS: &str = "   8       0 sda 100 0 2000 50 200 0 4000 80 0 10 130\n   8       1 sda1 90 0 1800 40 190 0 3900 70 0 9 110\n 259       0 nvme0n1 5 0 80 1 6 0 160 2 0 1 3\n 259       1 nvme0n1p1 4 0 70 1 5 0 150 2 0 1 3\n   7       0 loop0 1 0 8 0 0 0 0 0 0 0 0\n 253       0 dm-0 1 0 8 0 0 0 0 0 0 0 0\n";
    const DF: &str = "Filesystem     1024-blocks     Used Available Capacity Mounted on\n/dev/sda1         102400    40960     51200      45% /\ntmpfs              8000        0      8000       0% /dev/shm\nudev               4000        0      4000       0% /dev\n/dev/sdb1         204800   102400    102400      50% /mnt/my data\n";
    const PS_FULL: &str = "  PID  PPID USER     %CPU %MEM   RSS     ELAPSED STAT COMMAND COMMAND\n    1     0 root      0.1  0.2  4096  1-02:03:04 Ss   systemd /sbin/init splash\n 4321  1234 www-data 12.5  1.0 20480       00:42 S    nginx   nginx: worker process\n";
    const PS_MIN: &str = "PID   PPID  USER     RSS  ELAPSED STAT COMMAND ARGS\n    1     0 root     3000 00:10  S    sleep   sleep 1000\n";

    #[test]
    fn stat_and_cpu_percent() {
        let (t1, c1) = parse_stat(STAT1);
        let (t2, c2) = parse_stat(STAT2);
        assert_eq!(t1, Some((1000, 800)));
        assert_eq!(c1.len(), 2);
        // total: 1000 -> 1300 тиков, простой 800 -> 900: занято 200 из 300
        assert!((cpu_pct(t1.unwrap(), t2.unwrap()) - 200.0 / 3.0).abs() < 1e-9);
        assert!((cpu_pct(c1[0], c2[0]) - 100.0).abs() < 1e-9);
        assert!((cpu_pct(c1[1], c2[1]) - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(cpu_pct((10, 5), (10, 5)), 0.0);
    }

    #[test]
    fn meminfo() {
        let m = parse_meminfo(MEMINFO);
        assert_eq!(m.total, 16_000_000 * 1024);
        assert_eq!(m.available, 8_000_000 * 1024);
        assert_eq!(m.used, 8_000_000 * 1024);
        assert_eq!(m.swap_used, 500_000 * 1024);
        assert_eq!(m.cached, 4_100_000 * 1024);
        // без MemAvailable
        let m = parse_meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 100 kB\nCached: 300 kB\n");
        assert_eq!(m.available, 500 * 1024);
    }

    #[test]
    fn small_files() {
        assert_eq!(parse_loadavg("0.52 0.40 0.31 2/345 6789\n"), [0.52, 0.40, 0.31]);
        assert_eq!(parse_uptime("12345.67 99999.00\n"), 12345.67);
        assert_eq!(parse_os_release("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 22.04.3 LTS\"\nVERSION_ID=\"22.04\"\n"), "Ubuntu 22.04.3 LTS");
        assert_eq!(parse_os_release("NAME=Alpine\nVERSION_ID=3.19.1\n"), "Alpine 3.19.1");
    }

    #[test]
    fn net_dev_skips_loopback_and_handles_glued_columns() {
        assert_eq!(parse_net_dev(NETDEV), vec![("eth0".to_string(), 5_000_000, 250_000)]);
    }

    #[test]
    fn diskstats_drops_partitions_and_virtual() {
        let d = parse_diskstats(DISKSTATS);
        let names: Vec<_> = d.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["sda", "nvme0n1"]);
        assert_eq!((d[0].1, d[0].2), (2000 * 512, 4000 * 512));
    }

    #[test]
    fn df_filters_pseudo_and_keeps_spaces_in_mount() {
        let f = parse_df(DF);
        assert_eq!(f.iter().map(|x| x.mount.as_str()).collect::<Vec<_>>(), ["/", "/mnt/my data"]);
        assert_eq!(f[0].total, 102400 * 1024);
        assert!((f[0].percent - 40960.0 / (40960.0 + 51200.0) * 100.0).abs() < 1e-9);
    }

    #[test]
    fn ps_full_and_min() {
        let p = parse_ps("full", PS_FULL);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].pid, p[0].user.as_str(), p[0].etime.as_str(), p[0].comm.as_str()), (1, "root", "1-02:03:04", "systemd"));
        assert_eq!(p[0].args, "/sbin/init splash");
        assert_eq!((p[1].cpu, p[1].rss, p[1].args.as_str()), (12.5, 20480 * 1024, "nginx: worker process"));
        let p = parse_ps("min", PS_MIN);
        assert_eq!((p[0].pid, p[0].rss, p[0].comm.as_str(), p[0].args.as_str()), (1, 3000 * 1024, "sleep", "sleep 1000"));
    }

    fn feed(c: &mut Collector, uptime: &str, stat: &str, net_rx: u64, first: bool) -> Snapshot {
        let mut out = String::from("@@SG:BEGIN\n");
        if first {
            out += "@@SG:host\nweb1\n@@SG:osrel\nPRETTY_NAME=\"Debian 12\"\n@@SG:kernel\n6.1.0\n";
        }
        out += &format!("@@SG:stat\n{stat}@@SG:meminfo\n{MEMINFO}@@SG:loadavg\n1.0 2.0 3.0 1/2 3\n@@SG:uptime\n{uptime} 1.0\n");
        out += &format!("@@SG:net/dev\n  eth0: {net_rx} 1 0 0 0 0 0 0 {} 1 0 0 0 0 0 0\n@@SG:diskstats\n{DISKSTATS}@@SG:df\n{DF}@@SG:ps\nfull\n{PS_FULL}@@SG:END\n", net_rx / 2);
        out.lines().filter_map(|l| c.push_line(l)).next().expect("снимок")
    }

    #[test]
    fn collector_computes_rates_from_two_snapshots() {
        let mut c = Collector::new();
        let a = feed(&mut c, "100.00", STAT1, 1000, true);
        assert!(!a.rates_valid);
        assert_eq!((a.info.hostname.as_str(), a.info.os.as_str(), a.info.kernel.as_str(), a.info.cores), ("web1", "Debian 12", "6.1.0", 2));
        let b = feed(&mut c, "102.00", STAT2, 5000, false);
        assert!(b.rates_valid);
        assert_eq!(b.info.hostname, "web1"); // статика сохраняется
        assert!((b.cpu.total - 200.0 / 3.0).abs() < 1e-9);
        assert_eq!(b.cpu.cores.len(), 2);
        assert!((b.net_rx_bps - 2000.0).abs() < 1e-9);
        assert!((b.net_tx_bps - 1000.0).abs() < 1e-9);
        assert_eq!(b.processes.len(), 2);
        assert_eq!(b.filesystems.len(), 2);
        assert_eq!(b.load, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn unsupported_os_is_reported() {
        let mut c = Collector::new();
        assert!(c.push_line("@@SG:UNSUPPORTED Darwin").is_none());
        assert_eq!(c.unsupported.as_deref(), Some("Darwin"));
    }

    #[test]
    fn script_is_quotable_and_interval_clamped() {
        assert!(!SCRIPT.contains('\''));
        let c = command(100);
        assert!(c.contains("sleep 0.500 2>/dev/null || sleep 1"));
        assert!(command(2000).contains("sleep 2.000 2>/dev/null || sleep 2"));
    }

    #[test]
    fn kill_validation() {
        assert_eq!(kill_command(42, "term").unwrap(), "kill -s TERM -- 42 2>&1");
        assert!(kill_command(42, "SIGKILL").is_ok());
        assert!(kill_command(42, "TERM; rm -rf /").is_err());
        assert!(kill_command(1, "KILL").is_err());
    }
}

#[cfg(test)]
mod local {
    /// Прогон скрипта на локальной машине (если это Linux): проверяет вариант ps «full» с GNU procps.
    #[test]
    fn script_runs_locally() {
        if !cfg!(target_os = "linux") {
            return;
        }
        let out = std::process::Command::new("sh").arg("-c").arg(super::command(500).replacen("exec sh -c ", "timeout 1.2 sh -c ", 1)).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let mut c = super::Collector::new();
        let snaps: Vec<_> = text.lines().filter_map(|l| c.push_line(l)).collect();
        assert!(snaps.len() >= 2, "{text}");
        assert!(!snaps[1].processes.is_empty() && snaps[1].info.cores >= 1 && snaps[1].mem.total > 0);
    }
}
