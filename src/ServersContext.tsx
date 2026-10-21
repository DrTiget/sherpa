import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import { errorText, type ApiError, type Server } from "./types";

export type UnknownHost = Extract<ApiError, { kind: "unknown_host" }>;

interface Ctx {
  servers: Server[];
  loaded: boolean;
  connected: Set<string>;
  refresh: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  /** Показывает диалог подтверждения отпечатка; после «Доверять» сохраняет ключ и вызывает retry. */
  requestTrust: (err: UnknownHost, retry: () => void | Promise<void>) => void;
}

const ServersCtx = createContext<Ctx | null>(null);

export function useServers(): Ctx {
  const c = useContext(ServersCtx);
  if (!c) throw new Error("ServersProvider отсутствует");
  return c;
}

export function ServersProvider({ children }: { children: ReactNode }) {
  const [servers, setServers] = useState<Server[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [connected, setConnected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<{ err: UnknownHost; retry: () => void | Promise<void> } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => setConnected(new Set(await api.status())), []);
  const refresh = useCallback(async () => {
    setServers(await api.list());
    setLoaded(true);
    await refreshStatus();
  }, [refreshStatus]);

  useEffect(() => {
    refresh().catch((e) => setError(errorText(e)));
    const t = setInterval(() => refreshStatus().catch(() => {}), 5000);
    return () => clearInterval(t);
  }, [refresh, refreshStatus]);

  const requestTrust = useCallback((err: UnknownHost, retry: () => void | Promise<void>) => setPending({ err, retry }), []);

  const confirm = async () => {
    if (!pending) return;
    const { err, retry } = pending;
    setPending(null);
    try { await api.trustHost(err.host, err.port, err.fingerprint); await retry(); }
    catch (e) { setError(errorText(e)); }
  };

  const value = useMemo(() => ({ servers, loaded, connected, refresh, refreshStatus, requestTrust }), [servers, loaded, connected, refresh, refreshStatus, requestTrust]);

  return (
    <ServersCtx.Provider value={value}>
      {children}
      {pending && (
        <div className="modal-bg top">
          <div className="modal">
            <h3>Неизвестный хост</h3>
            <p>Впервые подключаемся к <b>{pending.err.host}:{pending.err.port}</b>. Сверьте отпечаток ключа ({pending.err.algorithm}) с тем, что показывает сервер, прежде чем доверять:</p>
            <code className="fp">{pending.err.fingerprint}</code>
            <div className="actions">
              <button onClick={() => setPending(null)}>Отмена</button>
              <button className="primary" onClick={confirm}>Доверять и продолжить</button>
            </div>
          </div>
        </div>
      )}
      {error && (
        <div className="modal-bg top" onClick={() => setError(null)}>
          <div className="modal"><h3>Ошибка</h3><p>{error}</p><div className="actions"><button onClick={() => setError(null)}>OK</button></div></div>
        </div>
      )}
    </ServersCtx.Provider>
  );
}
