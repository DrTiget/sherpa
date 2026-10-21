import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { useServers } from "../ServersContext";
import { DesktopContext, type DesktopCtx } from "./DesktopContext";
import { APPS, appById } from "./apps";
import { Icon } from "./icons";
import { focusedId, initialState, loadState, reducer, saveState } from "./reducer";
import Taskbar from "./Taskbar";
import Window from "./Window";
import type { Rect } from "./types";

export default function Desktop() {
  const { servers, loaded } = useServers();
  const [state, dispatch] = useReducer(reducer, initialState, () => loadState((id) => !!appById(id)));
  const deskEl = useRef<HTMLDivElement>(null);
  const [area, setArea] = useState({ w: 1000, h: 600 });
  const [snap, setSnap] = useState<Rect | null>(null);
  const [selIcon, setSelIcon] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    const el = deskEl.current!;
    const ro = new ResizeObserver(() => setArea({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setArea({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // сохранение раскладки (позиции/размеры, открытые приложения, активный сервер)
  useEffect(() => { const t = setTimeout(() => saveState(state), 200); return () => clearTimeout(t); }, [state]);

  // закрытые окна удалённых серверов не восстанавливаем
  useEffect(() => { if (loaded) dispatch({ type: "prune", serverIds: servers.map((s) => s.id) }); }, [loaded, servers]);

  useEffect(() => { if (!toast) return; const t = setTimeout(() => setToast(null), 3500); return () => clearTimeout(t); }, [toast]);

  const stateRef = useRef(state);
  stateRef.current = state;
  const areaRef = useRef(area);
  areaRef.current = area;

  const openApp = useCallback((appId: string, explicit?: string | null) => {
    const app = appById(appId);
    if (!app) return;
    const s = stateRef.current;
    let serverId: string | null = null;
    if (app.needsServer) {
      serverId = explicit ?? s.activeServerId ?? (servers.length === 1 ? servers[0].id : null);
      if (!serverId) {
        setToast("Сначала выберите сервер (в панели задач или в приложении «Серверы»)");
        if (!s.windows.some((w) => w.appId === "servers")) dispatch({ type: "open", id: crypto.randomUUID(), app: APPS[0], serverId: null, area: areaRef.current });
        return;
      }
      dispatch({ type: "activeServer", id: serverId });
    }
    if (app.singleInstancePerServer) {
      const ex = s.windows.find((w) => w.appId === appId && w.serverId === serverId);
      if (ex) { dispatch({ type: "focus", id: ex.id }); return; }
    }
    dispatch({ type: "open", id: crypto.randomUUID(), app, serverId, area: areaRef.current });
  }, [servers]);

  const setActiveServer = useCallback((id: string | null) => dispatch({ type: "activeServer", id }), []);
  const fid = focusedId(state);
  const ctx: DesktopCtx = useMemo(() => ({ state, dispatch, focusedId: fid, openApp, setActiveServer }), [state, fid, openApp, setActiveServer]);
  const deskRect = useCallback(() => deskEl.current?.getBoundingClientRect() ?? null, []);

  // первый запуск: пустой стол → открыть «Серверы»
  const first = useRef(true);
  useEffect(() => {
    if (first.current && loaded) {
      first.current = false;
      if (stateRef.current.windows.length === 0) openApp("servers");
    }
  }, [loaded, openApp]);

  return (
    <DesktopContext.Provider value={ctx}>
      <div className="desktop-root">
        <div className="desk" ref={deskEl} onPointerDown={(e) => { if (e.target === e.currentTarget) setSelIcon(null); }}>
          <div className="icons" onPointerDown={(e) => { if (e.target === e.currentTarget) setSelIcon(null); }}>
            {APPS.map((a) => (
              <div key={a.id} className={"dicon" + (selIcon === a.id ? " sel" : "")} onClick={() => setSelIcon(a.id)} onDoubleClick={() => openApp(a.id)}>
                <Icon name={a.icon} size={34} />
                <span>{a.title}</span>
              </div>
            ))}
          </div>
          {state.windows.map((w) => {
            const app = appById(w.appId);
            if (!app) return null;
            return (
              <Window key={w.id} win={w} app={app} area={area} focused={fid === w.id} deskRect={deskRect} dispatch={dispatch}
                serverName={w.serverId ? servers.find((s) => s.id === w.serverId)?.name ?? "?" : null} onSnapPreview={setSnap} />
            );
          })}
          {snap && <div className="snap-preview" style={{ left: snap.x, top: snap.y, width: snap.w, height: snap.h }} />}
          {toast && <div className="toast">{toast}</div>}
        </div>
        <Taskbar />
      </div>
    </DesktopContext.Provider>
  );
}
