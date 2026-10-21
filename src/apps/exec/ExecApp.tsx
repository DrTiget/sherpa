import { useServers } from "../../ServersContext";
import type { AppProps } from "../../desktop/types";
import ExecPanel from "./ExecPanel";

export default function ExecApp({ serverId }: AppProps) {
  const { servers, requestTrust } = useServers();
  const server = servers.find((s) => s.id === serverId);
  if (!server) return <div className="empty">Сервер не найден</div>;
  return (
    <div className="app-pad">
      <ExecPanel server={server} onError={(e) => requestTrust(e as Parameters<typeof requestTrust>[0], () => {})} />
    </div>
  );
}
