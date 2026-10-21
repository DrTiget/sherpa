import { invoke } from "@tauri-apps/api/core";
import type { ExecResult, Server, ServerInput } from "./types";

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
};
