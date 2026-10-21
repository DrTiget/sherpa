import { useState } from "react";
import { api } from "../../api";
import { errorText, type ExecResult, type Server } from "../../types";

interface Props {
  server: Server;
  onError: (e: unknown) => void;
}

export default function ExecPanel({ server, onError }: Props) {
  const [cmd, setCmd] = useState("uname -a");
  const [res, setRes] = useState<ExecResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true); setErr(null);
    try {
      setRes(await api.exec(server.id, cmd));
    } catch (x) {
      setRes(null);
      // неизвестный хост обрабатывает родитель (диалог отпечатка)
      if ((x as { kind?: string })?.kind === "unknown_host") onError(x); else setErr(errorText(x));
    } finally { setBusy(false); }
  };

  return (
    <div className="exec">
      <h3>Выполнить команду</h3>
      <form className="row" onSubmit={run}>
        <input className="grow mono" value={cmd} onChange={(e) => setCmd(e.target.value)} spellCheck={false} />
        <button className="primary" disabled={busy || !cmd.trim()}>{busy ? "Выполняется…" : "Выполнить"}</button>
      </form>
      {err && <div className="banner error">{err}</div>}
      {res && (
        <div className="out">
          <div className={res.exit_code === 0 ? "ok" : "bad"}>код выхода: {res.exit_code ?? "—"}</div>
          {res.stdout && <><div className="lbl">stdout</div><pre>{res.stdout}</pre></>}
          {res.stderr && <><div className="lbl">stderr</div><pre className="stderr">{res.stderr}</pre></>}
        </div>
      )}
    </div>
  );
}
