import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { errorText } from "../../types";
import { monitorApi } from "./api";
import { Chart } from "./Chart";
import { bytes, rate, uptime } from "./format";
import type { Process, Snapshot } from "./types";
import "./monitor.css";

export interface MonitorAppProps {
  serverId: string;
  windowId: string;
  setTitle: (t: string) => void;
}

const HISTORY_MS = 5 * 60 * 1000;
const INTERVALS = [1000, 2000, 5000, 10000];

/** История: [t(с), ряд1, ряд2, …] для каждого графика. */
interface History { cpu: number[][]; mem: number[][]; net: number[][]; disk: number[][] }
const emptyHistory = (): History => ({ cpu: [[], []], mem: [[], [], []], net: [[], [], []], disk: [[], [], []] });

function push(rows: number[][], vals: number[], max: number) {
  vals.forEach((v, i) => rows[i].push(v));
  if (rows[0].length > max) rows.forEach((r) => r.splice(0, r.length - max));
}

type SortKey = keyof Pick<Process, "pid" | "user" | "cpu" | "mem" | "rss" | "etime" | "state" | "comm">;

const CPU_S = [{ label: "CPU %", color: "#38bdf8", fill: true }];
const MEM_S = [{ label: "RAM", color: "#a78bfa", fill: true }, { label: "Swap", color: "#fbbf24" }];
const NET_S = [{ label: "Приём", color: "#34d399", fill: true }, { label: "Отдача", color: "#f472b6" }];
const DISK_S = [{ label: "Чтение", color: "#38bdf8", fill: true }, { label: "Запись", color: "#fb923c" }];

export default function MonitorApp({ serverId, setTitle }: MonitorAppProps) {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [hist, setHist] = useState<History>(emptyHistory);
  const [intervalMs, setIntervalMs] = useState(2000);
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [closed, setClosed] = useState<string | null>(null);
  const [epoch, setEpoch] = useState(0); // меняется для перезапуска монитора
  const pausedRef = useRef(false);
  pausedRef.current = paused;
  const histRef = useRef<History>(emptyHistory());

  useEffect(() => {
    let id: string | null = null;
    let dead = false;
    const max = Math.ceil(HISTORY_MS / intervalMs);
    histRef.current = emptyHistory();
    setError(null);
    setClosed(null);
    monitorApi
      .start(serverId, intervalMs, (e) => {
        if (dead) return;
        if (e.event === "error") { setError(e.message); return; }
        if (e.event === "closed") { setClosed(e.reason); return; }
        if (pausedRef.current) return;
        const t = e.ts_ms / 1000, h = histRef.current;
        if (e.rates_valid) {
          const pct = (u: number, tot: number) => (tot > 0 ? (u / tot) * 100 : 0);
          push(h.cpu, [t, e.cpu.total], max);
          push(h.mem, [t, pct(e.mem.used, e.mem.total), pct(e.mem.swap_used, e.mem.swap_total)], max);
          push(h.net, [t, e.net_rx_bps, e.net_tx_bps], max);
          push(h.disk, [t, e.disk_read_bps, e.disk_write_bps], max);
          // новые массивы, чтобы график увидел изменение
          setHist({ cpu: h.cpu.map((r) => r.slice()), mem: h.mem.map((r) => r.slice()), net: h.net.map((r) => r.slice()), disk: h.disk.map((r) => r.slice()) });
        }
        setSnap(e);
      })
      .then((mid) => {
        if (dead) monitorApi.stop(mid).catch(() => {});
        else id = mid;
      })
      .catch((err) => { if (!dead) setError(errorText(err)); });
    return () => { dead = true; if (id) monitorApi.stop(id).catch(() => {}); };
  }, [serverId, intervalMs, epoch]);

  useEffect(() => {
    if (snap?.info.hostname) setTitle(`Монитор — ${snap.info.hostname}`);
  }, [snap?.info.hostname, setTitle]);

  return (
    <div className="mon">
      <div className="mon-head">
        <div className="mon-host">
          <b>{snap?.info.hostname ?? "…"}</b>
          <span className="mon-sub">{snap?.info.os}</span>
        </div>
        <Stat k="Ядро" v={snap?.info.kernel} />
        <Stat k="Ядер" v={snap?.info.cores} />
        <Stat k="Аптайм" v={snap && uptime(snap.uptime_s)} />
        <Stat k="Load" v={snap && snap.load.map((l) => l.toFixed(2)).join(" ")} />
        <div className="mon-spacer" />
        <label className="mon-ctl">Интервал
          <select value={intervalMs} onChange={(e) => { setHist(emptyHistory()); setIntervalMs(+e.target.value); }}>
            {INTERVALS.map((i) => <option key={i} value={i}>{i / 1000} с</option>)}
          </select>
        </label>
        <button onClick={() => setPaused((p) => !p)}>{paused ? "▶ Продолжить" : "⏸ Пауза"}</button>
      </div>

      {error && <div className="mon-banner err">{error}</div>}
      {closed && !error && (
        <div className="mon-banner">Мониторинг остановлен ({closed}). <button onClick={() => { setHist(emptyHistory()); setEpoch((n) => n + 1); }}>Перезапустить</button></div>
      )}

      <div className="mon-body">
        <div className="mon-charts">
          <Card title="CPU" value={snap && `${snap.cpu.total.toFixed(0)}%`}>
            <Chart data={hist.cpu} series={CPU_S} range={[0, 100]} fmt={(v) => `${v.toFixed(0)}%`} />
            {snap && <div className="mon-cores">{snap.cpu.cores.map((c, i) => (
              <div className="mon-core" key={i} title={`Ядро ${i}: ${c.toFixed(0)}%`}><i style={{ height: `${c}%` }} /></div>
            ))}</div>}
          </Card>
          <Card title="Память" value={snap && `${bytes(snap.mem.used)} / ${bytes(snap.mem.total)}`}
            sub={snap && snap.mem.swap_total > 0 ? `swap ${bytes(snap.mem.swap_used)} / ${bytes(snap.mem.swap_total)}` : undefined}>
            <Chart data={hist.mem} series={MEM_S} range={[0, 100]} fmt={(v) => `${v.toFixed(0)}%`} />
          </Card>
          <Card title="Сеть" value={snap && `↓ ${rate(snap.net_rx_bps)}  ↑ ${rate(snap.net_tx_bps)}`}
            sub={snap?.net.map((n) => n.name).join(", ")}>
            <Chart data={hist.net} series={NET_S} fmt={rate} />
          </Card>
          <Card title="Диск" value={snap && `R ${rate(snap.disk_read_bps)}  W ${rate(snap.disk_write_bps)}`}
            sub={snap?.disks.map((d) => d.name).join(", ")}>
            <Chart data={hist.disk} series={DISK_S} fmt={rate} />
          </Card>
        </div>

        <div className="mon-section">
          <h4>Файловые системы</h4>
          <table className="mon-table">
            <thead><tr><th>Устройство</th><th>Точка монтирования</th><th className="r">Размер</th><th className="r">Занято</th><th className="bar-col">Заполнение</th></tr></thead>
            <tbody>
              {snap?.filesystems.map((f) => (
                <tr key={f.device + f.mount}>
                  <td className="mono">{f.device}</td><td className="mono">{f.mount}</td>
                  <td className="r">{bytes(f.total)}</td><td className="r">{bytes(f.used)}</td>
                  <td><div className="mon-bar" title={`${f.percent.toFixed(0)}%`}><i className={f.percent >= 90 ? "bad" : f.percent >= 75 ? "warn" : ""} style={{ width: `${f.percent}%` }} /><span>{f.percent.toFixed(0)}%</span></div></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <ProcessTable serverId={serverId} procs={snap?.processes ?? []} cores={snap?.info.cores ?? 1} />
      </div>
    </div>
  );
}

function Stat({ k, v }: { k: string; v: string | number | null | undefined | false }) {
  return <div className="mon-stat"><span>{k}</span><b>{v || "—"}</b></div>;
}

function Card({ title, value, sub, children }: { title: string; value?: string | null | false; sub?: string; children: React.ReactNode }) {
  return (
    <div className="mon-card">
      <div className="mon-card-head"><b>{title}</b><span>{value}</span></div>
      {sub && <div className="mon-sub">{sub}</div>}
      <div className="mon-card-body">{children}</div>
    </div>
  );
}

function ProcessTable({ serverId, procs, cores }: { serverId: string; procs: Process[]; cores: number }) {
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  const [confirm, setConfirm] = useState<{ p: Process; signal: "TERM" | "KILL" } | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const rows = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = q ? procs.filter((p) => `${p.pid} ${p.user} ${p.comm} ${p.args}`.toLowerCase().includes(q)) : procs.slice();
    const { key, desc } = sort;
    list.sort((a, b) => {
      const x = a[key], y = b[key];
      const c = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y));
      return desc ? -c : c;
    });
    return list;
  }, [procs, filter, sort]);

  const th = (key: SortKey, label: string, cls = "") => (
    <th className={`sortable ${cls}`} onClick={() => setSort((s) => ({ key, desc: s.key === key ? !s.desc : key !== "user" && key !== "comm" }))}>
      {label}{sort.key === key ? (sort.desc ? " ▼" : " ▲") : ""}
    </th>
  );

  const doKill = useCallback(async () => {
    if (!confirm) return;
    setBusy(true);
    try {
      await monitorApi.kill(serverId, confirm.p.pid, confirm.signal);
      setMsg({ ok: true, text: `Сигнал ${confirm.signal} отправлен процессу ${confirm.p.comm} (${confirm.p.pid})` });
    } catch (e) {
      setMsg({ ok: false, text: `Не удалось завершить ${confirm.p.comm} (${confirm.p.pid}): ${errorText(e)}` });
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  }, [confirm, serverId]);

  return (
    <div className="mon-section mon-procs">
      <div className="mon-proc-head">
        <h4>Процессы <span className="mon-sub">{rows.length} из {procs.length}</span></h4>
        <input placeholder="Фильтр: имя, PID, пользователь, команда" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      {msg && <div className={`mon-banner ${msg.ok ? "ok" : "err"}`} onClick={() => setMsg(null)}>{msg.text}</div>}
      <div className="mon-proc-scroll">
        <table className="mon-table">
          <thead><tr>
            {th("pid", "PID", "r")}{th("user", "Пользователь")}{th("cpu", "CPU %", "r")}{th("mem", "MEM %", "r")}
            {th("rss", "RSS", "r")}{th("etime", "Время")}{th("state", "Сост.")}{th("comm", "Команда")}<th />
          </tr></thead>
          <tbody>
            {rows.map((p) => (
              <tr key={p.pid}>
                <td className="r mono">{p.pid}</td><td>{p.user}</td>
                <td className={`r ${p.cpu >= 50 * Math.min(cores, 2) ? "hot" : ""}`}>{p.cpu.toFixed(1)}</td>
                <td className="r">{p.mem.toFixed(1)}</td><td className="r">{bytes(p.rss)}</td>
                <td className="mono">{p.etime}</td><td className="mono">{p.state}</td>
                <td className="mon-cmd" title={p.args}>{p.args}</td>
                <td className="r"><button className="mon-kill" title="Завершить" onClick={() => setConfirm({ p, signal: "TERM" })}>✕</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {confirm && (
        <div className="mon-modal" onClick={() => !busy && setConfirm(null)}>
          <div className="mon-dialog" onClick={(e) => e.stopPropagation()}>
            <h4>Завершить процесс?</h4>
            <p><b>{confirm.p.comm}</b> (PID {confirm.p.pid}, {confirm.p.user})</p>
            <p className="mon-sub mono">{confirm.p.args.slice(0, 200)}</p>
            <label className="mon-ctl">Сигнал
              <select value={confirm.signal} onChange={(e) => setConfirm({ ...confirm, signal: e.target.value as "TERM" | "KILL" })}>
                <option value="TERM">TERM — корректное завершение</option>
                <option value="KILL">KILL — принудительно</option>
              </select>
            </label>
            <div className="mon-actions">
              <button onClick={() => setConfirm(null)} disabled={busy}>Отмена</button>
              <button className="danger" onClick={doKill} disabled={busy}>{busy ? "…" : "Завершить"}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
