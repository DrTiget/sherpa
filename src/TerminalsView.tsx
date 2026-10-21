import { useState } from "react";
import TerminalTab from "./TerminalTab";
import type { ApiError, Server, TermTab } from "./types";

interface Props {
  tabs: TermTab[];
  activeKey: string | null;
  servers: Server[];
  onSelect: (key: string) => void;
  onClose: (key: string) => void;
  onRename: (key: string, title: string) => void;
  onServerChanged: () => Promise<void>;
  onUnknownHost: (err: Extract<ApiError, { kind: "unknown_host" }>, retry: () => void) => void;
}

export default function TerminalsView({ tabs, activeKey, servers, onSelect, onClose, onRename, onServerChanged, onUnknownHost }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  return (
    <div className="terms">
      <div className="tabs">
        {tabs.map((t) => (
          <div key={t.key} className={"tab" + (t.key === activeKey ? " active" : "")} onClick={() => onSelect(t.key)}
            onDoubleClick={() => setEditing(t.key)} title={servers.find((s) => s.id === t.serverId)?.host}>
            {editing === t.key ? (
              <input autoFocus defaultValue={t.title} onClick={(e) => e.stopPropagation()}
                onBlur={(e) => { onRename(t.key, e.target.value.trim() || t.title); setEditing(null); }}
                onKeyDown={(e) => { if (e.key === "Enter" || e.key === "Escape") (e.target as HTMLInputElement).blur(); }} />
            ) : <span>{t.title}</span>}
            <b className="x" onClick={(e) => { e.stopPropagation(); onClose(t.key); }}>×</b>
          </div>
        ))}
      </div>
      {tabs.length === 0 && <div className="empty">Нет открытых терминалов. Нажмите «Терминал» у сервера.</div>}
      {tabs.map((t) => {
        const srv = servers.find((x) => x.id === t.serverId);
        return srv ? (
          <TerminalTab key={t.key} server={srv} active={t.key === activeKey} onServerChanged={onServerChanged} onUnknownHost={onUnknownHost} />
        ) : null;
      })}
    </div>
  );
}
