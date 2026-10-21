export interface HostInfo { hostname: string; os: string; kernel: string; cores: number }
export interface NetIf { name: string; rx_bps: number; tx_bps: number; rx_total: number; tx_total: number }
export interface DiskDev { name: string; read_bps: number; write_bps: number }
export interface Filesystem { device: string; mount: string; total: number; used: number; available: number; percent: number }
export interface Process {
  pid: number; ppid: number; user: string; cpu: number; mem: number; rss: number;
  etime: string; state: string; comm: string; args: string;
}
export interface Snapshot {
  ts_ms: number;
  rates_valid: boolean;
  info: HostInfo;
  uptime_s: number;
  load: [number, number, number];
  cpu: { total: number; cores: number[] };
  mem: { total: number; used: number; available: number; buffers: number; cached: number; swap_total: number; swap_used: number };
  net: NetIf[];
  net_rx_bps: number;
  net_tx_bps: number;
  disks: DiskDev[];
  disk_read_bps: number;
  disk_write_bps: number;
  filesystems: Filesystem[];
  processes: Process[];
}
export type MonitorEvent =
  | ({ event: "snapshot" } & Snapshot)
  | { event: "error"; message: string }
  | { event: "closed"; reason: string };
