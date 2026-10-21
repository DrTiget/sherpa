import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { AuthType, Server, ServerInput } from "./types";

interface Props {
  server: Server | null;
  groups: string[];
  onSave: (input: ServerInput) => Promise<void>;
  onCancel: () => void;
}

export default function ServerForm({ server, groups, onSave, onCancel }: Props) {
  const a = server?.auth;
  const [name, setName] = useState(server?.name ?? "");
  const [host, setHost] = useState(server?.host ?? "");
  const [port, setPort] = useState(server?.port ?? 22);
  const [username, setUsername] = useState(server?.username ?? "root");
  const [group, setGroup] = useState(server?.group ?? "");
  const [type, setType] = useState<AuthType>(a?.type ?? "password");
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState(a?.type === "key" ? a.key_path ?? "" : "");
  const [keyPem, setKeyPem] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const editing = !!server;
  const hasSaved = (a?.type === "password" && a.password_ref) || (a?.type === "key" && a.passphrase_ref);

  const pick = async () => {
    const f = await open({ multiple: false, directory: false, title: "Приватный ключ" });
    if (typeof f === "string") { setKeyPath(f); setKeyPem(""); }
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    const nz = (s: string) => (s ? s : null);
    const auth: ServerInput["auth"] =
      type === "password" ? { type, password: nz(password) }
      : type === "key" ? { type, key_path: nz(keyPath), key_pem: nz(keyPem), passphrase: nz(passphrase) }
      : { type };
    try {
      await onSave({ name, host, port, username, group: nz(group.trim()), auth });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="form" onSubmit={submit}>
      <h2>{editing ? "Редактирование сервера" : "Новый сервер"}</h2>
      <label>Название<input required value={name} onChange={(e) => setName(e.target.value)} placeholder="prod-web-1" /></label>
      <div className="row">
        <label className="grow">Хост<input required value={host} onChange={(e) => setHost(e.target.value)} placeholder="example.com" /></label>
        <label className="narrow">Порт<input type="number" min={1} max={65535} required value={port} onChange={(e) => setPort(Number(e.target.value))} /></label>
      </div>
      <div className="row">
        <label className="grow">Пользователь<input required value={username} onChange={(e) => setUsername(e.target.value)} /></label>
        <label className="grow">Группа<input list="groups" value={group} onChange={(e) => setGroup(e.target.value)} placeholder="без группы" />
          <datalist id="groups">{groups.map((g) => <option key={g} value={g} />)}</datalist>
        </label>
      </div>
      <label>Аутентификация
        <select value={type} onChange={(e) => setType(e.target.value as AuthType)}>
          <option value="password">Пароль</option>
          <option value="key">Приватный ключ</option>
          <option value="agent">SSH-agent</option>
        </select>
      </label>
      {type === "password" && (
        <label>Пароль<input type="password" value={password} onChange={(e) => setPassword(e.target.value)}
          placeholder={hasSaved ? "сохранён — оставьте пустым, чтобы не менять" : ""} autoComplete="off" /></label>
      )}
      {type === "key" && (
        <>
          <label>Файл ключа
            <div className="row">
              <input className="grow" value={keyPath} onChange={(e) => { setKeyPath(e.target.value); setKeyPem(""); }} placeholder="~/.ssh/id_ed25519" />
              <button type="button" onClick={pick}>Выбрать…</button>
            </div>
          </label>
          <label>…или вставьте ключ (будет сохранён в системное хранилище)
            <textarea rows={4} value={keyPem} onChange={(e) => { setKeyPem(e.target.value); if (e.target.value) setKeyPath(""); }}
              placeholder={a?.type === "key" && a.key_ref ? "импортированный ключ сохранён" : "-----BEGIN OPENSSH PRIVATE KEY-----"} />
          </label>
          <label>Passphrase (если ключ зашифрован)
            <input type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)}
              placeholder={a?.type === "key" && a.passphrase_ref ? "сохранена — оставьте пустым, чтобы не менять" : ""} autoComplete="off" /></label>
        </>
      )}
      {type === "agent" && <p className="hint">Ключи берутся из запущенного ssh-agent (SSH_AUTH_SOCK / Pageant).</p>}
      <div className="actions">
        <button type="button" onClick={onCancel}>Отмена</button>
        <button className="primary" disabled={busy}>{editing ? "Сохранить" : "Добавить"}</button>
      </div>
    </form>
  );
}
