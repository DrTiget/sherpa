import MonitorApp from "../apps/monitor/MonitorApp";
import ExecApp from "../apps/exec/ExecApp";
import ServersApp from "../apps/servers/ServersApp";
import TerminalApp from "../apps/terminal/TerminalApp";
import { createElement } from "react";
import type { AppDef, AppProps } from "./types";

const MonitorWindow = (p: AppProps) => (p.serverId ? createElement(MonitorApp, { ...p, serverId: p.serverId }) : null);

/**
 * Реестр приложений рабочего стола. Новое приложение = одна запись здесь.
 * Компонент получает props { serverId, windowId, setTitle } (см. AppProps).
 */
export const APPS: AppDef[] = [
  { id: "servers", title: "Серверы", icon: "servers", component: ServersApp, defaultSize: { w: 880, h: 560 }, singleInstancePerServer: true },
  { id: "terminal", title: "Терминал", icon: "terminal", component: TerminalApp, defaultSize: { w: 780, h: 480 }, needsServer: true },
  { id: "exec", title: "Выполнить команду", icon: "run", component: ExecApp, defaultSize: { w: 640, h: 440 }, needsServer: true },
  { id: "monitor", title: "Монитор ресурсов", icon: "monitor", component: MonitorWindow, defaultSize: { w: 820, h: 560 }, needsServer: true, singleInstancePerServer: true },
];

export const appById = (id: string): AppDef | undefined => APPS.find((a) => a.id === id);
