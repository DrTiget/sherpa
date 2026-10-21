import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import ExecPanel from "./ExecPanel";
import ServerForm from "./ServerForm";
import TerminalsView from "./TerminalsView";
import { errorText, type ApiError, type Server, type ServerInput, type TermTab } from "./types";

type Mode = { t: "view" } | { t: "new" } | { t: "edit" };
type Pending = { err: Extract<ApiError, { kind: "unknown_host" }>; retry: () => Promise<void> };

export default function App() {
  const [servers, setServers] = useState<Server[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>({ t: "view" });
  const [connected, setConnected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);

  const [view, setView] = useState<"servers" | "terminals">("servers");
  const [tabs, setTabs] = useState<TermTab[]>([]);
  const [activeTab, setActiveTab] = useState<string | null>(null);

  const openTerminal = (srv: Server) => {
    const n = tabs.filter((t) => t.serverId === srv.id).length;
    const key = crypto.randomUUID();
    setTabs((t) => [...t, { key, serverId: srv.id, title: n ? `${srv.name} (${n + 1})` : srv.name }]);
    setActiveTab(key); setView("terminals");
  };
  const closeTab = (key: string) => {
    const rest = tabs.filter((t) => t.key !== key);
    setTabs(rest);
    if (activeTab === key) setActiveTab(rest.length ? rest[rest.length - 1].key : null);
  };
  const onUnknownHost = useCallback((err: Extract<ApiError, { kind: "unknown_host" }>, retry: () => void) => {
    setPending({ err, retry: async () => retry() });
  }, []);

  const refresh = useCallback(async () => {
    setServers(await api.list());
    setConnected(new Set(await api.status()));
  }, []);

  useEffect(() => {
    refresh().catch((e) => setMsg({ kind: "error", text: errorText(e) }));
    const t = setInterval(() => api.status().then((s) => setConnected(new Set(s))).catch(() => {}), 5000);
    return () => clearInterval(t);
  }, [refresh]);

  const current = servers.find((s) => s.id === selected) ?? null;
  const groups = useMemo(() => [...new Set(servers.map((s) => s.group).filter((g): g is string => !!g))].sort(), [servers]);
  const byGroup = useMemo(() => {
    const m = new Map<string, Server[]>();
    for (const s of servers) m.set(s.group ?? "", [...(m.get(s.group ?? "") ?? []), s]);
    return [...m.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  }, [servers]);

  /** Выполняет действие; при первом контакте с хостом показывает диалог отпечатка и повторяет действие после подтверждения. */
  const guarded = useCallback(async (label: string, action: () => Promise<string | void>) => {
    setBusy(label); setMsg(null);
    try {
      const ok = await action();
      if (ok) setMsg({ kind: "ok", text: ok });
    } catch (e) {
      const err = e as ApiError;
      if (err?.kind === "unknown_host") setPending({ err, retry: () => guarded(label, action) });
      else setMsg({ kind: "error", text: errorText(e) });
    } finally {
      setBusy(null);
      api.status().then((s) => setConnected(new Set(s))).catch(() => {});
    }
  }, []);

  const save = async (input: ServerInput) => {
    try {
      const s = mode.t === "edit" && current ? await api.update(current.id, input) : await api.create(input);
      await refresh();
      setSelected(s.id); setMode({ t: "view" });
    } catch (e) { setMsg({ kind: "error", text: errorText(e) }); }
  };

  const remove = async () => {
    if (!current || !confirm(`Удалить сервер «${current.name}»?`)) return;
    try { await api.remove(current.id); setSelected(null); await refresh(); }
    catch (e) { setMsg({ kind: "error", text: errorText(e) }); }
  };

  const confirmFp = async () => {
    if (!pending) return;
    const { err, retry } = pending;
    setPending(null);
    try { await api.trustHost(err.host, err.port, err.fingerprint); await retry(); }
    catch (e) { setMsg({ kind: "error", text: errorText(e) }); }
  };

  const isConn = current ? connected.has(current.id) : false;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">ServerGUI</div>
        <button className="primary add" onClick={() => { setMode({ t: "new" }); setSelected(null); setMsg(null); }}>+ Добавить сервер</button>
        <div className="views">
          <button className={view === "servers" ? "sel" : ""} onClick={() => setView("servers")}>Серверы</button>
          <button className={view === "terminals" ? "sel" : ""} onClick={() => setView("terminals")}>Терминалы{tabs.length ? ` (${tabs.length})` : ""}</button>
        </div>
        <div className="list">
          {servers.length === 0 && <p className="hint">Серверов пока нет</p>}
          {byGroup.map(([g, list]) => (
            <div key={g}>
              <div className="group">{g || "Без группы"}</div>
              {list.map((s) => (
                <div key={s.id} className={"item" + (s.id === selected ? " active" : "")}
                  onClick={() => { setSelected(s.id); setMode({ t: "view" }); setMsg(null); }}>
                  <span className={"dot" + (connected.has(s.id) ? " on" : "")} title={connected.has(s.id) ? "подключено" : "не подключено"} />
                  <span className="name">{s.name}</span>
                  <span className="sub">{s.host}<b className="term-btn" title="Открыть терминал" onClick={(e) => { e.stopPropagation(); openTerminal(s); }}>&gt;_</b></span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </aside>

      <main className="main" style={{ display: view === "terminals" ? "none" : undefined }}>
        {msg && <div className={"banner " + msg.kind} onClick={() => setMsg(null)}>{msg.text}</div>}
        {mode.t !== "view" ? (
          <ServerForm key={mode.t + (current?.id ?? "")} server={mode.t === "edit" ? current : null} groups={groups}
            onSave={save} onCancel={() => setMode({ t: "view" })} />
        ) : current ? (
          <>
            <div className="head">
              <div>
                <h2>{current.name}</h2>
                <div className="sub">{current.username}@{current.host}:{current.port} · {current.auth.type === "password" ? "пароль" : current.auth.type === "key" ? "ключ" : "ssh-agent"}</div>
              </div>
              <span className={"status" + (isConn ? " on" : "")}>{isConn ? "● подключено" : "○ не подключено"}</span>
            </div>
            <div className="toolbar">
              <button disabled={!!busy} onClick={() => guarded("test", async () => { await api.test(current.id); return "Подключение успешно"; })}>
                {busy === "test" ? "Проверка…" : "Проверить подключение"}
              </button>
              {isConn ? (
                <button disabled={!!busy} onClick={() => guarded("disc", async () => { await api.disconnect(current.id); })}>Отключиться</button>
              ) : (
                <button className="primary" disabled={!!busy} onClick={() => guarded("conn", async () => { await api.connect(current.id); return "Подключено"; })}>
                  {busy === "conn" ? "Подключение…" : "Подключиться"}
                </button>
              )}
              <span className="spacer" />
              <button className="primary" onClick={() => openTerminal(current)}>Терминал</button>
              <button onClick={() => setMode({ t: "edit" })}>Изменить</button>
              <button className="danger" onClick={remove}>Удалить</button>
            </div>
            <ExecPanel key={current.id} server={current} onError={(e) => {
              const err = e as Extract<ApiError, { kind: "unknown_host" }>;
              setPending({ err, retry: async () => {} });
            }} />
          </>
        ) : (
          <div className="empty">Выберите сервер слева или добавьте новый</div>
        )}
      </main>
      <div className="main term-main" style={{ display: view === "terminals" ? "flex" : "none" }}>
        <TerminalsView tabs={tabs} activeKey={activeTab} servers={servers} onSelect={setActiveTab} onClose={closeTab}
          onRename={(k, title) => setTabs((ts) => ts.map((t) => (t.key === k ? { ...t, title } : t)))} onServerChanged={refresh} onUnknownHost={onUnknownHost} />
      </div>

      {pending && (
        <div className="modal-bg">
          <div className="modal">
            <h3>Неизвестный хост</h3>
            <p>Впервые подключаемся к <b>{pending.err.host}:{pending.err.port}</b>. Сверьте отпечаток ключа ({pending.err.algorithm}) с тем, что показывает сервер, прежде чем доверять:</p>
            <code className="fp">{pending.err.fingerprint}</code>
            <div className="actions">
              <button onClick={() => setPending(null)}>Отмена</button>
              <button className="primary" onClick={confirmFp}>Доверять и продолжить</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
