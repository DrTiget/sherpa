import { Channel, invoke } from "@tauri-apps/api/core";
import type { ExecResult, Server, ServerInput, ShellStatus, TermEvent, TerminalInfo } from "./types";

export const api = {
  list: () => invoke<Server[]>("list_servers"),
  create: (input: ServerInput) => invoke<Server>("create_server", { input }),
  update: (id: string, input: ServerInput) => invoke<Server>("update_server", { id, input }),
  remove: (id: string) => invoke<void>("delete_server", { id }),
  test: (id: string) => invoke<void>("test_connection", { id }),
  connect: (id: string) => invoke<void>("connect", { id }),
  disconnect: (id: string) => invoke<void>("disconnect", { id }),
  status: () => invoke<string[]>("connection_status"),
  exec: (id: string, command: string) => invoke<ExecResult>("exec", { id, command }),
  trustHost: (host: string, port: number, fingerprint: string) =>
    invoke<void>("trust_host", { host, port, fingerprint }),
  terminalOpen: (serverId: string, cols: number, rows: number, onEvent: (e: TermEvent) => void) => {
    const ch = new Channel<TermEvent>();
    ch.onmessage = onEvent;
    return invoke<TerminalInfo>("terminal_open", { serverId, cols, rows, onEvent: ch });
  },
  terminalWrite: (terminalId: string, data: Uint8Array) =>
    invoke<void>("terminal_write", { terminalId, data: Array.from(data) }),
  terminalResize: (terminalId: string, cols: number, rows: number) =>
    invoke<void>("terminal_resize", { terminalId, cols, rows }),
  terminalClose: (terminalId: string) => invoke<void>("terminal_close", { terminalId }),
  setEnhanced: (id: string, enabled: boolean) => invoke<ShellStatus | null>("set_enhanced_shell", { id, enabled }),
  uninstallShell: (id: string) => invoke<void>("uninstall_enhanced_shell", { id }),
};
