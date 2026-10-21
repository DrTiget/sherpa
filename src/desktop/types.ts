import type { ComponentType } from "react";

export interface Rect { x: number; y: number; w: number; h: number }

/** Props, которые получает компонент любого приложения-окна. */
export interface AppProps {
  serverId: string | null;
  windowId: string;
  setTitle: (title: string) => void;
}

export type IconName = "servers" | "terminal" | "run" | "monitor" | "app";

export interface AppDef {
  id: string;
  title: string;
  icon: IconName;
  component: ComponentType<AppProps>;
  defaultSize: { w: number; h: number };
  /** Приложению нужен выбранный сервер (иначе serverId = null). */
  needsServer?: boolean;
  /** Не открывать второе окно для той же пары (приложение, сервер) — вместо этого фокусировать существующее. */
  singleInstancePerServer?: boolean;
  minSize?: { w: number; h: number };
}

export interface WinState {
  id: string;
  appId: string;
  serverId: string | null;
  /** Заголовок, заданный приложением через setTitle; null — название приложения. */
  title: string | null;
  rect: Rect;
  /** Прежний прямоугольник, пока окно развёрнуто или прижато к краю (snap). */
  restore: Rect | null;
  maximized: boolean;
  minimized: boolean;
  z: number;
}

export interface DesktopState {
  windows: WinState[];
  zTop: number;
  activeServerId: string | null;
}

export const MIN_W = 320;
export const MIN_H = 200;
