export type AuthType = "password" | "key" | "agent";

export type AuthConfig =
  | { type: "password"; password_ref: string | null }
  | { type: "key"; key_path: string | null; key_ref: string | null; passphrase_ref: string | null }
  | { type: "agent" };

export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  group: string | null;
  auth: AuthConfig;
  enhanced_shell: boolean;
}

export type AuthInput =
  | { type: "password"; password: string | null }
  | { type: "key"; key_path: string | null; key_pem: string | null; passphrase: string | null }
  | { type: "agent" };

export interface ServerInput {
  name: string;
  host: string;
  port: number;
  username: string;
  group: string | null;
  auth: AuthInput;
  enhanced_shell: boolean;
}

export interface ExecResult {
  stdout: string;
  stderr: string;
  exit_code: number | null;
}

/** Ошибка бэкенда (CoreError). */
export type ApiError =
  | { kind: "message"; message: string }
  | { kind: "not_found" }
  | { kind: "unknown_host"; host: string; port: number; fingerprint: string; algorithm: string }
  | { kind: "host_key_mismatch"; host: string; port: number; expected: string; actual: string }
  | { kind: "auth_failed"; message: string };

export function errorText(e: unknown): string {
  const err = e as ApiError;
  switch (err?.kind) {
    case "message": return err.message;
    case "not_found": return "Сервер не найден";
    case "auth_failed": return `Ошибка аутентификации: ${err.message}`;
    case "unknown_host": return `Неизвестный хост ${err.host}:${err.port}`;
    case "host_key_mismatch":
      return `Ключ хоста ${err.host}:${err.port} изменился (было ${err.expected}, стало ${err.actual}). Подключение отклонено: возможна подмена сервера (MITM).`;
    default: return typeof e === "string" ? e : JSON.stringify(e);
  }
}

export type TermEvent =
  | { event: "data"; data: number[] }
  | { event: "closed"; exit_code: number | null; reason: string };

export interface TermTab {
  key: string;
  serverId: string;
  title: string;
}

export interface TerminalInfo {
  terminal_id: string;
  /** "zsh" | "bash" | "default" */
  shell: string;
  install_hint: string | null;
}

export interface ShellStatus {
  shell: string;
  uploaded: boolean;
  install_hint: string | null;
}
