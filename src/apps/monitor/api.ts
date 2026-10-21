import { Channel, invoke } from "@tauri-apps/api/core";
import type { MonitorEvent } from "./types";

export const monitorApi = {
  start: (serverId: string, intervalMs: number, onEvent: (e: MonitorEvent) => void) => {
    const ch = new Channel<MonitorEvent>();
    ch.onmessage = onEvent;
    return invoke<string>("monitor_start", { serverId, intervalMs, onEvent: ch });
  },
  stop: (monitorId: string) => invoke<void>("monitor_stop", { monitorId }),
  kill: (serverId: string, pid: number, signal: "TERM" | "KILL") => invoke<void>("process_kill", { serverId, pid, signal }),
};
