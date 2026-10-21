import { useCallback, useMemo, useState } from "react";
import { api } from "../../api";
import { useDesktop } from "../../desktop/DesktopContext";
import type { AppProps } from "../../desktop/types";
import { useServers } from "../../ServersContext";
import { errorText, type ApiError, type Server, type ServerInput } from "../../types";
import ServerForm from "./ServerForm";

type Mode = "view" | "new" | "edit";

export default function ServersApp(_: AppProps) {
  const { servers, connected, refresh, refreshStatus, requestTrust } = useServers();
  const { state, openApp, setActiveServer } = useDesktop();
  const [mode, setMode] = useState<Mode>("view");
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const selected = state.activeServerId;
  const current = servers.find((s) => s.id === selected) ?? null;
  const groups = useMemo(() => [...new Set(servers.map((s) => s.group).filter((g): g is string => !!g))].sort(), [servers]);
  const byGroup = useMemo(() => {
    const m = new Map<string, Server[]>();
    for (const s of servers) m.set(s.group ?? "", [...(m.get(s.group ?? "") ?? []), s]);
    return [...m.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  }, [servers]);

  /** Выполняет действие; при первом контакте с хостом показывает диалог отпечатка и повторяет действие после подтверждения. */
  const guarded = useCallback(async (label: string, action: () => Promise<string | void>): Promise<void> => {
    setBusy(label); setMsg(null);
    try {
      const ok = await action();
      if (ok) setMsg({ kind: "ok", text: ok });
    } catch (e) {
      const err = e as ApiError;
      if (err?.kind === "unknown_host") requestTrust(err, () => guarded(label, action));
      else setMsg({ kind: "error", text: errorText(e) });
    } finally {
      setBusy(null);
      refreshStatus().catch(() => {});
    }
  }, [requestTrust, refreshStatus]);

  const save = async (input: ServerInput) => {
    try {
      const s = mode === "edit" && current ? await api.update(current.id, input) : await api.create(input);
      await refresh();
      setActiveServer(s.id); setMode("view");
    } catch (e) { setMsg({ kind: "error", text: errorText(e) }); }
  };

  const remove = async () => {
    if (!current || !confirm(`Удалить сервер «${current.name}»?`)) return;
    try { await api.remove(current.id); setActiveServer(null); await refresh(); }
    catch (e) { setMsg({ kind: "error", text: errorText(e) }); }
  };

  const isConn = current ? connected.has(current.id) : false;

  return (
    <div className="srv-app">
      <aside className="sidebar">
        <button className="primary add" onClick={() => { setMode("new"); setMsg(null); }}>+ Добавить сервер</button>
        <div className="list">
          {servers.length === 0 && <p className="hint">Серверов пока нет</p>}
          {byGroup.map(([g, list]) => (
            <div key={g}>
              <div className="group">{g || "Без группы"}</div>
              {list.map((s) => (
                <div key={s.id} className={"item" + (s.id === selected ? " active" : "")}
                  onClick={() => { setActiveServer(s.id); setMode("view"); setMsg(null); }}
                  onDoubleClick={() => openApp("terminal", s.id)} title="Двойной клик — открыть терминал">
                  <span className={"dot" + (connected.has(s.id) ? " on" : "")} title={connected.has(s.id) ? "подключено" : "не подключено"} />
                  <span className="name">{s.name}</span>
                  <span className="sub">{s.host}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </aside>

      <main className="main">
        {msg && <div className={"banner " + msg.kind} onClick={() => setMsg(null)}>{msg.text}</div>}
        {mode !== "view" ? (
          <ServerForm key={mode + (current?.id ?? "")} server={mode === "edit" ? current : null} groups={groups} onSave={save} onCancel={() => setMode("view")} />
        ) : current ? (
          <>
            <div className="head">
              <div>
                <h2>{current.name}</h2>
                <div className="sub">{current.username}@{current.host}:{current.port} · {current.auth.type === "password" ? "пароль" : current.auth.type === "key" ? "ключ" : "ssh-agent"}</div>
              </div>
              <span className={"status" + (isConn ? " on" : "")}>{isConn ? "● подключено" : "○ не подключено"}</span>
            </div>
            <div className="toolbar wrap">
              <button className="primary" onClick={() => openApp("terminal", current.id)}>Терминал</button>
              <button onClick={() => openApp("exec", current.id)}>Выполнить команду</button>
              <button disabled={!!busy} onClick={() => guarded("test", async () => { await api.test(current.id); return "Подключение успешно"; })}>
                {busy === "test" ? "Проверка…" : "Проверить подключение"}
              </button>
              {isConn ? (
                <button disabled={!!busy} onClick={() => guarded("disc", async () => { await api.disconnect(current.id); })}>Отключиться</button>
              ) : (
                <button disabled={!!busy} onClick={() => guarded("conn", async () => { await api.connect(current.id); return "Подключено"; })}>
                  {busy === "conn" ? "Подключение…" : "Подключиться"}
                </button>
              )}
              <span className="spacer" />
              <button onClick={() => setMode("edit")}>Изменить</button>
              <button className="danger" onClick={remove}>Удалить</button>
            </div>
          </>
        ) : (
          <div className="empty">Выберите сервер слева или добавьте новый</div>
        )}
      </main>
    </div>
  );
}
