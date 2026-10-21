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
  onUnknownHost: (err: Extract<ApiError, { kind: "unknown_host" }>, retry: () => void) => void;
}

export default function TerminalsView({ tabs, activeKey, servers, onSelect, onClose, onRename, onUnknownHost }: Props) {
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
      {tabs.map((t) => (
        <TerminalTab key={t.key} serverId={t.serverId} active={t.key === activeKey} onUnknownHost={onUnknownHost} />
      ))}
    </div>
  );
}
