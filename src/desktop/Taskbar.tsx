import { useEffect, useState } from "react";
import { useServers } from "../ServersContext";
import { useDesktop } from "./DesktopContext";
import { APPS, appById } from "./apps";
import { Icon } from "./icons";
import StartMenu from "./StartMenu";

function Clock() {
  const [now, setNow] = useState(new Date());
  useEffect(() => { const t = setInterval(() => setNow(new Date()), 10_000); return () => clearInterval(t); }, []);
  return (
    <div className="clock" title={now.toLocaleString("ru-RU")}>
      <div>{now.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</div>
      <div className="date">{now.toLocaleDateString("ru-RU")}</div>
    </div>
  );
}

export default function Taskbar() {
  const { state, dispatch, focusedId, setActiveServer } = useDesktop();
  const { servers, connected } = useServers();
  const [menu, setMenu] = useState(false);
  const online = servers.filter((s) => connected.has(s.id));
  const nameOf = (id: string | null) => servers.find((s) => s.id === id)?.name;

  return (
    <div className="taskbar">
      <button className={"start" + (menu ? " on" : "")} onClick={() => setMenu((m) => !m)}>
        <Icon name="app" size={16} /> Пуск
      </button>
      {menu && <StartMenu onClose={() => setMenu(false)} />}
      <div className="tasks">
        {state.windows.map((w) => {
          const app = appById(w.appId) ?? APPS[0];
          const active = focusedId === w.id;
          return (
            <button key={w.id} className={"task" + (active ? " active" : "") + (w.minimized ? " min" : "")}
              title={`${w.title ?? app.title}${w.serverId ? " — " + nameOf(w.serverId) : ""}`}
              onClick={() => dispatch({ type: active ? "minimize" : "focus", id: w.id })}>
              <Icon name={app.icon} size={14} />
              <span>{w.title ?? app.title}{w.serverId ? ` · ${nameOf(w.serverId) ?? "?"}` : ""}</span>
            </button>
          );
        })}
      </div>
      <label className="active-server" title="Сервер, для которого открываются приложения">
        Сервер:
        <select value={state.activeServerId ?? ""} onChange={(e) => setActiveServer(e.target.value || null)}>
          <option value="">не выбран</option>
          {servers.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </label>
      <div className="tray-status" title={online.length ? "Подключены: " + online.map((s) => s.name).join(", ") : "Нет активных подключений"}>
        <span className={"dot" + (online.length ? " on" : "")} /> {online.length}/{servers.length}
      </div>
      <Clock />
    </div>
  );
}
