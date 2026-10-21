import { useServers } from "../../ServersContext";
import { useWindowFocused } from "../../desktop/DesktopContext";
import type { AppProps } from "../../desktop/types";
import TerminalTab from "./TerminalTab";

/** Одно окно — один терминал (на сервере открывается свой PTY-канал). */
export default function TerminalApp({ serverId, windowId }: AppProps) {
  const { servers, refresh, requestTrust } = useServers();
  const focused = useWindowFocused(windowId);
  const server = servers.find((s) => s.id === serverId);
  if (!server) return <div className="empty">Сервер не найден</div>;
  return <TerminalTab server={server} active={focused} onServerChanged={refresh} onUnknownHost={requestTrust} />;
}
