import { createContext, useContext } from "react";
import type { Dispatch } from "react";
import type { Action } from "./reducer";
import type { DesktopState } from "./types";

export interface DesktopCtx {
  state: DesktopState;
  dispatch: Dispatch<Action>;
  focusedId: string | null;
  /** Открывает приложение; сервер — явный, иначе активный. Для приложений с needsServer без сервера покажет подсказку. */
  openApp: (appId: string, serverId?: string | null) => void;
  setActiveServer: (id: string | null) => void;
}

export const DesktopContext = createContext<DesktopCtx | null>(null);

export function useDesktop(): DesktopCtx {
  const c = useContext(DesktopContext);
  if (!c) throw new Error("DesktopContext отсутствует");
  return c;
}

/** Окно приложения в фокусе и не свёрнуто. */
export function useWindowFocused(windowId: string): boolean {
  const d = useDesktop();
  return d.focusedId === windowId && !d.state.windows.find((w) => w.id === windowId)?.minimized;
}
