import Desktop from "./desktop/Desktop";
import { ServersProvider } from "./ServersContext";

export default function App() {
  return (
    <ServersProvider>
      <Desktop />
    </ServersProvider>
  );
}
