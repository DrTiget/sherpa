import { useState } from "react";
import { useServers } from "../ServersContext";
import { useDesktop } from "./DesktopContext";
import { APPS } from "./apps";
import { Icon } from "./icons";

export default function StartMenu({ onClose }: { onClose: () => void }) {
  const { openApp, state } = useDesktop();
  const { servers } = useServers();
  const [hover, setHover] = useState<string | null>(null);

  const open = (appId: string, serverId?: string) => { openApp(appId, serverId); onClose(); };
  return (
    <>
      <div className="menu-shield" onClick={onClose} />
      <div className="startmenu">
        <div className="menu-head">Приложения</div>
        {APPS.map((a) => (
          <div key={a.id} className="menu-item-wrap" onMouseEnter={() => setHover(a.needsServer ? a.id : null)}>
            <button className="menu-item" onClick={() => open(a.id)}>
              <Icon name={a.icon} />
              <span>{a.title}</span>
              {a.needsServer && <span className="arrow">▸</span>}
            </button>
            {a.needsServer && hover === a.id && (
              <div className="submenu">
                <div className="menu-head">{a.title} для сервера</div>
                {servers.length === 0 && <div className="hint pad">Серверов нет</div>}
                {servers.map((s) => (
                  <button key={s.id} className="menu-item" onClick={() => open(a.id, s.id)}>
                    <span className={"dot" + (state.activeServerId === s.id ? " on" : "")} />
                    <span>{s.name}</span>
                    <span className="sub">{s.host}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </>
  );
}
