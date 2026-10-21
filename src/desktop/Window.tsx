import { Suspense, useCallback, useRef, type PointerEvent as RPE } from "react";
import { Icon } from "./icons";
import { clampRect, type Action, type Area } from "./reducer";
import { MIN_H, MIN_W, type AppDef, type Rect, type WinState } from "./types";

type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
const EDGES: Edge[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];
const SNAP_PX = 6;

interface Props {
  win: WinState;
  app: AppDef;
  serverName: string | null;
  area: Area;
  focused: boolean;
  deskRect: () => DOMRect | null;
  dispatch: (a: Action) => void;
  onSnapPreview: (r: Rect | null) => void;
}

/** Куда прижимать окно при перетаскивании к краю стола. */
export function snapTarget(cx: number, cy: number, area: Area): { kind: "max" | "left" | "right"; rect: Rect } | null {
  if (cy <= SNAP_PX) return { kind: "max", rect: { x: 0, y: 0, w: area.w, h: area.h } };
  if (cx <= SNAP_PX) return { kind: "left", rect: { x: 0, y: 0, w: Math.floor(area.w / 2), h: area.h } };
  if (cx >= area.w - SNAP_PX) return { kind: "right", rect: { x: Math.floor(area.w / 2), y: 0, w: Math.ceil(area.w / 2), h: area.h } };
  return null;
}

export default function Window({ win, app, serverName, area, focused, deskRect, dispatch, onSnapPreview }: Props) {
  const min = app.minSize ?? { w: MIN_W, h: MIN_H };
  const rect: Rect = win.maximized ? { x: 0, y: 0, w: area.w, h: area.h } : win.rect;
  const rectRef = useRef(rect);
  rectRef.current = rect;
  const winRef = useRef(win);
  winRef.current = win;
  const Comp = app.component;
  const setTitle = useCallback((title: string) => dispatch({ type: "title", id: win.id, title }), [dispatch, win.id]);

  const rel = (e: { clientX: number; clientY: number }) => {
    const d = deskRect();
    return { x: e.clientX - (d?.left ?? 0), y: e.clientY - (d?.top ?? 0) };
  };

  const onTitleDown = (e: RPE<HTMLDivElement>) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".wbtn")) return;
    dispatch({ type: "focus", id: win.id });
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const start = rel(e);
    let base = { ...rectRef.current };
    let origin = start;
    let docked = winRef.current.maximized || winRef.current.restore !== null;
    let snap: ReturnType<typeof snapTarget> = null;
    let moved = false;

    const move = (ev: PointerEvent) => {
      const p = rel(ev);
      if (!moved && Math.hypot(p.x - start.x, p.y - start.y) < 4) return;
      moved = true;
      if (docked) {
        // вытаскиваем из развёрнутого/прижатого состояния: возвращаем прежний размер под курсор
        const r = winRef.current.restore ?? base;
        const ratio = Math.min(Math.max((start.x - base.x) / Math.max(base.w, 1), 0), 1);
        base = { x: p.x - r.w * ratio, y: 0, w: r.w, h: r.h };
        origin = p;
        docked = false;
      }
      const next = clampRect({ ...base, x: base.x + (p.x - origin.x), y: base.y + (p.y - origin.y) }, area, min);
      dispatch({ type: "setRect", id: win.id, rect: next });
      snap = snapTarget(p.x, p.y, area);
      onSnapPreview(snap?.rect ?? null);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      onSnapPreview(null);
      if (!snap) return;
      if (snap.kind === "max") {
        // сохранить прежний прямоугольник для восстановления
        dispatch({ type: "toggleMax", id: win.id });
      } else dispatch({ type: "snap", id: win.id, rect: snap.rect });
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const onEdgeDown = (edge: Edge) => (e: RPE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    dispatch({ type: "focus", id: win.id });
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const start = rel(e);
    const r0 = { ...rectRef.current };
    const move = (ev: PointerEvent) => {
      const p = rel(ev);
      const dx = p.x - start.x, dy = p.y - start.y;
      let { x, y, w, h } = r0;
      if (edge.includes("e")) w = r0.w + dx;
      if (edge.includes("s")) h = r0.h + dy;
      if (edge.includes("w")) { w = r0.w - dx; x = r0.x + dx; }
      if (edge.includes("n")) { h = r0.h - dy; y = r0.y + dy; }
      if (w < min.w) { if (edge.includes("w")) x = r0.x + r0.w - min.w; w = min.w; }
      if (h < min.h) { if (edge.includes("n")) y = r0.y + r0.h - min.h; h = min.h; }
      if (y < 0) { if (edge.includes("n")) h += y; y = 0; }
      if (edge.includes("e") && x + w > area.w) w = area.w - x;
      if (edge.includes("s") && y + h > area.h) h = area.h - y;
      dispatch({ type: "setRect", id: win.id, rect: { x, y, w: Math.max(w, min.w), h: Math.max(h, min.h) } });
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const title = win.title ?? app.title;
  return (
    <div
      className={"window" + (focused ? " focused" : "") + (win.maximized ? " maximized" : "")}
      style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, zIndex: win.z, display: win.minimized ? "none" : undefined }}
      onPointerDownCapture={() => dispatch({ type: "focus", id: win.id })}
    >
      <div className="wtitle" onPointerDown={onTitleDown} onDoubleClick={(e) => { if (!(e.target as HTMLElement).closest(".wbtn")) dispatch({ type: "toggleMax", id: win.id }); }}>
        <span className="wicon"><Icon name={app.icon} size={15} /></span>
        <span className="wname">{title}{serverName ? ` — ${serverName}` : ""}</span>
        <button className="wbtn" title="Свернуть" onClick={() => dispatch({ type: "minimize", id: win.id })}>—</button>
        <button className="wbtn" title={win.maximized ? "Восстановить" : "Развернуть"} onClick={() => dispatch({ type: "toggleMax", id: win.id })}>{win.maximized ? "❐" : "□"}</button>
        <button className="wbtn close" title="Закрыть" onClick={() => dispatch({ type: "close", id: win.id })}>✕</button>
      </div>
      <div className="wbody">
        <Suspense fallback={<div className="empty">Загрузка…</div>}>
          <Comp serverId={win.serverId} windowId={win.id} setTitle={setTitle} />
        </Suspense>
      </div>
      {!win.maximized && EDGES.map((e) => <div key={e} className={"edge edge-" + e} onPointerDown={onEdgeDown(e)} />)}
    </div>
  );
}
