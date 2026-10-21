import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { api } from "./api";
import { errorText, type ApiError } from "./types";

type Status = "connecting" | "open" | "ended";
type UnknownHost = Extract<ApiError, { kind: "unknown_host" }>;

interface Props {
  serverId: string;
  active: boolean;
  /** Первый контакт с хостом: родитель показывает диалог отпечатка и вызывает retry после подтверждения. */
  onUnknownHost: (err: UnknownHost, retry: () => void) => void;
}

const THEME = {
  background: "#0a0c10", foreground: "#e4e8ef", cursor: "#38bdf8", selectionBackground: "#23405a",
  black: "#1d222b", red: "#f87171", green: "#34d399", yellow: "#fbbf24", blue: "#38bdf8",
  magenta: "#c084fc", cyan: "#22d3ee", white: "#e4e8ef",
};

export default function TerminalTab({ serverId, active, onUnknownHost }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const search = useRef<SearchAddon | null>(null);
  const tid = useRef<string | null>(null);
  const gen = useRef(0); // отбрасывает события от прошлых сессий после «Переподключить»
  const [status, setStatus] = useState<Status>("connecting");
  const [info, setInfo] = useState("");
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");
  const decoder = useRef(new TextEncoder());

  const start = useCallback(async () => {
    const t = term.current!;
    const my = ++gen.current;
    setStatus("connecting"); setInfo("");
    try {
      const id = await api.terminalOpen(serverId, t.cols, t.rows, (ev) => {
        if (gen.current !== my) return;
        if (ev.event === "data") t.write(new Uint8Array(ev.data));
        else {
          tid.current = null;
          setStatus("ended");
          setInfo(ev.exit_code !== null ? `Сессия завершена (код выхода ${ev.exit_code})` : `Сессия завершена: ${ev.reason}`);
        }
      });
      if (gen.current !== my) { api.terminalClose(id).catch(() => {}); return; }
      tid.current = id;
      setStatus("open");
      t.focus();
    } catch (e) {
      if ((e as ApiError)?.kind === "unknown_host") {
        setInfo("Ожидается подтверждение отпечатка хоста…");
        setStatus("ended");
        onUnknownHost(e as UnknownHost, () => void start());
      } else { setStatus("ended"); setInfo(`Не удалось открыть терминал: ${errorText(e)}`); }
    }
  }, [serverId, onUnknownHost]);

  useEffect(() => {
    const t = new Terminal({
      fontFamily: 'ui-monospace, "Cascadia Code", Menlo, Consolas, monospace', fontSize: 13,
      cursorBlink: true, theme: THEME, scrollback: 10000, allowProposedApi: true,
    });
    const f = new FitAddon(), s = new SearchAddon();
    t.loadAddon(f); t.loadAddon(s); t.loadAddon(new WebLinksAddon());
    t.open(host.current!);
    term.current = t; fit.current = f; search.current = s;
    f.fit();

    t.onData((d) => { if (tid.current) api.terminalWrite(tid.current, decoder.current.encode(d)).catch(() => {}); });
    t.onResize(({ cols, rows }) => { if (tid.current) api.terminalResize(tid.current, cols, rows).catch(() => {}); });
    t.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || !e.ctrlKey || !e.shiftKey) return true;
      if (e.code === "KeyC") { const sel = t.getSelection(); if (sel) navigator.clipboard.writeText(sel); return false; }
      if (e.code === "KeyV") { navigator.clipboard.readText().then((x) => t.paste(x)).catch(() => {}); return false; }
      if (e.code === "KeyF") { setSearching(true); return false; }
      return true;
    });

    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => { if (host.current && host.current.offsetParent) try { f.fit(); } catch { /* скрыт */ } });
    });
    ro.observe(host.current!);
    void start();
    return () => {
      ro.disconnect(); cancelAnimationFrame(raf);
      gen.current++;
      if (tid.current) api.terminalClose(tid.current).catch(() => {});
      t.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (active) { requestAnimationFrame(() => { try { fit.current?.fit(); } catch { /* */ } term.current?.focus(); }); }
  }, [active]);

  const reconnect = () => { term.current?.write("\r\n\x1b[90m--- переподключение ---\x1b[0m\r\n"); void start(); };
  const find = (back: boolean) => query && (back ? search.current?.findPrevious(query) : search.current?.findNext(query));

  return (
    <div className="term-wrap" style={{ display: active ? "flex" : "none" }}>
      {searching && (
        <div className="term-search">
          <input autoFocus value={query} placeholder="Поиск" onChange={(e) => { setQuery(e.target.value); search.current?.findNext(e.target.value); }}
            onKeyDown={(e) => {
              if (e.key === "Enter") find(e.shiftKey);
              if (e.key === "Escape") { setSearching(false); search.current?.clearDecorations(); term.current?.focus(); }
            }} />
          <button onClick={() => find(true)}>↑</button><button onClick={() => find(false)}>↓</button>
          <button onClick={() => { setSearching(false); term.current?.focus(); }}>✕</button>
        </div>
      )}
      <div className="term-host" ref={host} />
      {status === "connecting" && <div className="term-bar">Подключение…</div>}
      {status === "ended" && (
        <div className="term-bar ended">
          <span>{info || "Сессия завершена"}</span>
          <button className="primary" onClick={reconnect}>Переподключить</button>
        </div>
      )}
    </div>
  );
}
