import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { api } from "../../api";
import { errorText, type ApiError, type Server } from "../../types";

type Status = "connecting" | "open" | "ended";
type UnknownHost = Extract<ApiError, { kind: "unknown_host" }>;

interface Props {
  server: Server;
  active: boolean;
  /** Перечитать список серверов (после смены флага «улучшенная оболочка»). */
  onServerChanged: () => Promise<void>;
  /** Первый контакт с хостом: родитель показывает диалог отпечатка и вызывает retry после подтверждения. */
  onUnknownHost: (err: UnknownHost, retry: () => void) => void;
}

// Палитра Kali (Tango)
const THEME = {
  background: "#1b1d21", foreground: "#d3d7cf", cursor: "#8ae234", cursorAccent: "#1b1d21", selectionBackground: "#3b4f6b",
  black: "#2e3436", red: "#cc0000", green: "#4e9a06", yellow: "#c4a000", blue: "#3465a4", magenta: "#75507b", cyan: "#06989a", white: "#d3d7cf",
  brightBlack: "#6b7280", brightRed: "#ef2929", brightGreen: "#8ae234", brightYellow: "#fce94f", brightBlue: "#729fcf",
  brightMagenta: "#ad7fa8", brightCyan: "#34e2e2", brightWhite: "#eeeeec",
};

export default function TerminalTab({ server, active, onServerChanged, onUnknownHost }: Props) {
  const serverId = server.id;
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
  const [hint, setHint] = useState<string | null>(null);
  const [toolBusy, setToolBusy] = useState(false);
  const [toolErr, setToolErr] = useState<string | null>(null);

  const start = useCallback(async () => {
    const t = term.current!;
    const my = ++gen.current;
    setStatus("connecting"); setInfo(""); setHint(null);
    try {
      const opened = await api.terminalOpen(serverId, t.cols, t.rows, (ev) => {
        if (gen.current !== my) return;
        if (ev.event === "data") t.write(new Uint8Array(ev.data));
        else {
          tid.current = null;
          setStatus("ended");
          setInfo(ev.exit_code !== null ? `Сессия завершена (код выхода ${ev.exit_code})` : `Сессия завершена: ${ev.reason}`);
        }
      });
      if (gen.current !== my) { api.terminalClose(opened.terminal_id).catch(() => {}); return; }
      tid.current = opened.terminal_id;
      setHint(opened.shell !== "zsh" ? opened.install_hint : null);
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

  /** Закрыть текущую сессию и открыть новую (например, после переключения оболочки). */
  const restart = () => {
    gen.current++;
    if (tid.current) { api.terminalClose(tid.current).catch(() => {}); tid.current = null; }
    term.current?.write("\r\n\x1b[90m--- новая сессия ---\x1b[0m\r\n");
    void start();
  };
  const tool = async (fn: () => Promise<void>) => {
    setToolBusy(true); setToolErr(null);
    try { await fn(); await onServerChanged(); restart(); } catch (e) { setToolErr(errorText(e)); } finally { setToolBusy(false); }
  };
  const toggleEnhanced = () => tool(async () => { await api.setEnhanced(serverId, !server.enhanced_shell); });
  const uninstall = () => {
    if (!confirm("Удалить ~/.sherpa с сервера (файлы оболочки и историю команд)? Опция будет выключена.")) return;
    void tool(() => api.uninstallShell(serverId));
  };
  const reconnect = () => { term.current?.write("\r\n\x1b[90m--- переподключение ---\x1b[0m\r\n"); void start(); };
  const find = (back: boolean) => query && (back ? search.current?.findPrevious(query) : search.current?.findNext(query));

  return (
    <div className="term-wrap">
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
      <div className="term-tools">
        <button disabled={toolBusy} onClick={toggleEnhanced} title="Подсказки, подсветка и автодополнение в стиле Kali (файлы в ~/.sherpa на сервере)">
          Улучшенная оболочка: {server.enhanced_shell ? "вкл" : "выкл"}
        </button>
        {server.enhanced_shell && <button disabled={toolBusy} onClick={uninstall}>Удалить с сервера</button>}
        {toolBusy && <span className="hint">применяется…</span>}
        {toolErr && <span className="tool-err">{toolErr}</span>}
      </div>
      {hint && server.enhanced_shell && (
        <div className="term-bar hintbar">
          <span>Установите zsh для подсказок и подсветки: <code>{hint}</code></span>
          <button onClick={() => navigator.clipboard.writeText(hint)}>Копировать</button>
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
