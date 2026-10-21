# Монитор ресурсов (`src/apps/monitor`)

Приложение для оконного менеджера: метрики сервера по SSH без агента (CPU, RAM/swap, сеть, диски, ФС, процессы).
Бэкенд — `core/src/monitor.rs`, команды Tauri `monitor_start` / `monitor_stop` / `process_kill`. Только Linux на сервере.

## Подключение

```tsx
import MonitorApp from "./apps/monitor/MonitorApp";

// родитель задаёт размер; компонент занимает 100% ширины и высоты
<div style={{ width: "100%", height: "100%" }}>
  <MonitorApp serverId={server.id} windowId={win.id} setTitle={(t) => setWindowTitle(win.id, t)} />
</div>
```

Props: `serverId` (id сервера из списка), `windowId`, `setTitle(t)` — вызывается с «Монитор — <hostname>».
Мониторинг стартует при монтировании и останавливается при размонтировании (а также при `disconnect` сервера).
Стили используют CSS-переменные приложения (`--bg`, `--panel`, `--panel2`, `--line`, `--text`, `--muted`, `--accent`, `--ok`, `--bad`) и общие `button`/`input`/`select`.
Зависимость: `uplot`.
