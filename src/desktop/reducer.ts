import type { AppDef, DesktopState, Rect, WinState } from "./types";
import { MIN_H, MIN_W } from "./types";

export type Action =
  | { type: "open"; id: string; app: AppDef; serverId: string | null; area: Area }
  | { type: "focus"; id: string }
  | { type: "close"; id: string }
  | { type: "minimize"; id: string }
  | { type: "toggleMax"; id: string }
  | { type: "setRect"; id: string; rect: Rect }
  | { type: "snap"; id: string; rect: Rect }
  | { type: "title"; id: string; title: string }
  | { type: "activeServer"; id: string | null }
  | { type: "prune"; serverIds: string[] };

export interface Area { w: number; h: number }

export const initialState: DesktopState = { windows: [], zTop: 1, activeServerId: null };

/** Окно не может уйти целиком за пределы стола: остаётся минимум 80px по горизонтали и заголовок по вертикали. */
export function clampRect(r: Rect, area: Area, min = { w: MIN_W, h: MIN_H }): Rect {
  const w = Math.max(min.w, r.w), h = Math.max(min.h, r.h);
  const x = Math.min(Math.max(r.x, 80 - w), Math.max(area.w - 80, 0));
  const y = Math.min(Math.max(r.y, 0), Math.max(area.h - 32, 0));
  return { x, y, w, h };
}

export function focusedId(s: DesktopState): string | null {
  let best: WinState | null = null;
  for (const w of s.windows) if (!w.minimized && (!best || w.z > best.z)) best = w;
  return best?.id ?? null;
}

const upd = (s: DesktopState, id: string, f: (w: WinState) => WinState): DesktopState => ({ ...s, windows: s.windows.map((w) => (w.id === id ? f(w) : w)) });

export function reducer(s: DesktopState, a: Action | { type: "load"; state: DesktopState }): DesktopState {
  switch (a.type) {
    case "load": return a.state;
    case "open": {
      const n = s.windows.length;
      const size = { w: Math.min(a.app.defaultSize.w, a.area.w - 20), h: Math.min(a.app.defaultSize.h, a.area.h - 20) };
      const rect = clampRect({ x: 110 + (n % 8) * 28, y: 30 + (n % 8) * 28, ...size }, a.area, a.app.minSize);
      const w: WinState = { id: a.id, appId: a.app.id, serverId: a.serverId, title: null, rect, restore: null, maximized: false, minimized: false, z: s.zTop + 1 };
      return { ...s, windows: [...s.windows, w], zTop: s.zTop + 1 };
    }
    case "focus": {
      const cur = s.windows.find((w) => w.id === a.id);
      if (!cur || (!cur.minimized && cur.z === s.zTop)) return s;
      return { ...upd(s, a.id, (w) => ({ ...w, minimized: false, z: s.zTop + 1 })), zTop: s.zTop + 1 };
    }
    case "close": return { ...s, windows: s.windows.filter((w) => w.id !== a.id) };
    case "minimize": return upd(s, a.id, (w) => ({ ...w, minimized: true }));
    case "toggleMax": {
      const next = upd(s, a.id, (w) => w.maximized
        ? { ...w, maximized: false, rect: w.restore ?? w.rect, restore: null }
        : { ...w, maximized: true, restore: w.restore ?? w.rect });
      return reducer(next, { type: "focus", id: a.id });
    }
    case "setRect": return upd(s, a.id, (w) => ({ ...w, rect: a.rect, restore: null, maximized: false }));
    case "snap": return upd(s, a.id, (w) => ({ ...w, restore: w.restore ?? w.rect, rect: a.rect, maximized: false }));
    case "title": return upd(s, a.id, (w) => (w.title === a.title ? w : { ...w, title: a.title }));
    case "activeServer": return { ...s, activeServerId: a.id };
    case "prune": {
      const ok = new Set(a.serverIds);
      const windows = s.windows.filter((w) => w.serverId === null || ok.has(w.serverId));
      const activeServerId = s.activeServerId && ok.has(s.activeServerId) ? s.activeServerId : null;
      return windows.length === s.windows.length && activeServerId === s.activeServerId ? s : { ...s, windows, activeServerId };
    }
  }
}

const KEY = "sherpa.desktop.v1";

export function loadState(known: (appId: string) => boolean): DesktopState {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null") as DesktopState | null;
    if (!raw || !Array.isArray(raw.windows)) return initialState;
    const windows = raw.windows.filter((w) => known(w.appId) && w.rect && typeof w.rect.x === "number");
    return { windows, zTop: Math.max(1, ...windows.map((w) => w.z), raw.zTop ?? 1), activeServerId: raw.activeServerId ?? null };
  } catch { return initialState; }
}

export function saveState(s: DesktopState) {
  try { localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* квота/приватный режим */ }
}
