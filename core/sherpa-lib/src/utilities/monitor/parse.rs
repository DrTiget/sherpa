//! Разбор /proc и ps для монитора. Чистые функции — тестируются без сервера.

use serde::Serialize;

/// Счётчики одной строки `cpu…` из /proc/stat.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

/// Строки `cpu` (сумма) и `cpuN` (ядра) — в порядке файла.
pub fn cpu_times(stat: &str) -> Vec<CpuTimes> {
    stat.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            // user nice system idle iowait irq softirq steal (guest уже входит в user)
            let total: u64 = v.iter().take(8).sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            CpuTimes { total, idle }
        })
        .collect()
}

/// Загрузка в процентах между двумя замерами.
pub fn usage_pct(prev: CpuTimes, cur: CpuTimes) -> Option<f64> {
    let dt = cur.total.checked_sub(prev.total)?;
    let di = cur.idle.checked_sub(prev.idle)?;
    if dt == 0 {
        return Some(0.0);
    }
    Some(round1(100.0 * (dt.saturating_sub(di)) as f64 / dt as f64))
}

pub fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub free_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    pub usage_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Swap {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_pct: f64,
}

/// /proc/meminfo → память и swap (значения в байтах).
pub fn meminfo(text: &str) -> (Memory, Swap) {
    let get = |key: &str| -> u64 {
        text.lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k.trim() == key)
                    .then(|| v.split_whitespace().next()?.parse::<u64>().ok())
                    .flatten()
            })
            .unwrap_or(0)
            * 1024
    };
    let total = get("MemTotal");
    let free = get("MemFree");
    let buffers = get("Buffers");
    let cached = get("Cached") + get("SReclaimable");
    let available = match get("MemAvailable") {
        0 => free + buffers + cached,
        a => a,
    };
    let used = total.saturating_sub(available);
    let pct = |u: u64, t: u64| {
        if t == 0 {
            0.0
        } else {
            round1(100.0 * u as f64 / t as f64)
        }
    };
    let swap_total = get("SwapTotal");
    let swap_used = swap_total.saturating_sub(get("SwapFree"));
    (
        Memory {
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
            free_bytes: free,
            buffers_bytes: buffers,
            cached_bytes: cached,
            usage_pct: pct(used, total),
        },
        Swap {
            total_bytes: swap_total,
            used_bytes: swap_used,
            usage_pct: pct(swap_used, swap_total),
        },
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Load {
    #[serde(rename = "1m")]
    pub one: f64,
    #[serde(rename = "5m")]
    pub five: f64,
    #[serde(rename = "15m")]
    pub fifteen: f64,
    pub running: u64,
    pub total: u64,
}

/// /proc/loadavg: `0.10 0.20 0.30 1/234 5678`.
pub fn loadavg(text: &str) -> Load {
    let f: Vec<&str> = text.split_whitespace().collect();
    let num = |i: usize| f.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let (running, total) = f
        .get(3)
        .and_then(|x| x.split_once('/'))
        .map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0)))
        .unwrap_or((0, 0));
    Load {
        one: num(0),
        five: num(1),
        fifteen: num(2),
        running,
        total,
    }
}

/// /proc/uptime: секунды с загрузки.
pub fn uptime(text: &str) -> u64 {
    text.split_whitespace()
        .next()
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0) as u64
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub rss_bytes: u64,
    pub elapsed_sec: u64,
    pub state: String,
    pub command: String,
}

/// `ps -eo pid=,ppid=,user=,pcpu=,pmem=,rss=,etimes=,stat=,comm=`.
pub fn processes(text: &str) -> Vec<Process> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            let user = it.next()?.to_string();
            let cpu_pct = it.next()?.parse().ok()?;
            let mem_pct = it.next()?.parse().ok()?;
            let rss_kb: u64 = it.next()?.parse().ok()?;
            let elapsed_sec = it.next()?.parse().ok()?;
            let state = it.next()?.to_string();
            let command = it.collect::<Vec<_>>().join(" ");
            Some(Process {
                pid,
                ppid,
                user,
                cpu_pct,
                mem_pct,
                rss_bytes: rss_kb * 1024,
                elapsed_sec,
                state,
                command,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod fixtures {
    pub const STAT1: &str =
        "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0 0 0\ncpu1 50 0 50 400 0 0 0 0 0 0\nintr 1 2 3\n";
    pub const STAT2: &str =
        "cpu  200 0 200 1000 0 0 0 0 0 0\ncpu0 150 0 100 450 0 0 0 0 0 0\ncpu1 50 0 100 550 0 0 0 0 0 0\nintr 1 2 3\n";
    pub const MEMINFO: &str = "MemTotal:        8000000 kB\nMemFree:         1000000 kB\nMemAvailable:    6000000 kB\nBuffers:          100000 kB\nCached:          2000000 kB\nSReclaimable:     200000 kB\nSwapTotal:       2000000 kB\nSwapFree:        1500000 kB\n";
    pub const LOADAVG: &str = "0.52 0.40 0.31 2/345 9876\n";
    pub const UPTIME: &str = "86461.25 170000.00\n";
    pub const PS: &str = "  812     1 mysql     12.5  4.1 335000 86400 Ssl  mysqld\n 1234   812 www-data   3.0  0.5  40000   120 S    php-fpm: pool www\n";
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn cpu() {
        let a = cpu_times(STAT1);
        let b = cpu_times(STAT2);
        assert_eq!(a.len(), 3);
        // total: 1000 → 1400 (Δ400), idle: 800 → 1000 (Δ200) → 50%
        assert_eq!(usage_pct(a[0], b[0]), Some(50.0));
        // ядро 0: Δtotal 200, Δidle 50 → 75%
        assert_eq!(usage_pct(a[1], b[1]), Some(75.0));
        assert_eq!(usage_pct(b[0], a[0]), None);
    }

    #[test]
    fn memory() {
        let (m, s) = meminfo(MEMINFO);
        assert_eq!(m.total_bytes, 8_000_000 * 1024);
        assert_eq!(m.available_bytes, 6_000_000 * 1024);
        assert_eq!(m.used_bytes, 2_000_000 * 1024);
        assert_eq!(m.usage_pct, 25.0);
        assert_eq!(m.cached_bytes, 2_200_000 * 1024);
        assert_eq!(s.used_bytes, 500_000 * 1024);
        assert_eq!(s.usage_pct, 25.0);
        // Старое ядро без MemAvailable.
        let (m, _) = meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 100 kB\nCached: 300 kB\n");
        assert_eq!(m.available_bytes, 500 * 1024);
    }

    #[test]
    fn load_uptime_ps() {
        let l = loadavg(LOADAVG);
        assert_eq!(
            (l.one, l.five, l.fifteen, l.running, l.total),
            (0.52, 0.40, 0.31, 2, 345)
        );
        assert_eq!(uptime(UPTIME), 86461);
        let p = processes(PS);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].command, "mysqld");
        assert_eq!(p[0].rss_bytes, 335000 * 1024);
        assert_eq!(p[1].command, "php-fpm: pool www");
        assert_eq!(p[1].ppid, 812);
        assert!(processes("garbage line\n").is_empty());
    }
}
