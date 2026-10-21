import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

export interface ChartSeries { label: string; color: string; fill?: boolean }

interface Props {
  /** [время в секундах, ряд1, ряд2, …] */
  data: number[][];
  series: ChartSeries[];
  /** Формат значений на оси и в подсказке. */
  fmt?: (v: number) => string;
  /** Фиксированный диапазон оси Y (например, [0, 100]); иначе авто от 0. */
  range?: [number, number];
}

const GRID = "#2a313c";
const TICK = "#8b95a5";

/** Лёгкая обёртка над uPlot: подстраивается под размер контейнера, данные обновляются через setData. */
export function Chart({ data, series, fmt = (v) => v.toFixed(0), range }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const latest = useRef(data);
  latest.current = data;

  useEffect(() => {
    const el = box.current!;
    const opts: uPlot.Options = {
      width: Math.max(el.clientWidth, 50),
      height: Math.max(el.clientHeight, 50),
      legend: { show: false },
      cursor: { drag: { x: false, y: false }, points: { size: 6 } },
      scales: { x: { time: true }, y: range ? { range } : { range: (_u, _min, max) => [0, Math.max(max * 1.15, 1)] } },
      axes: [
        { stroke: TICK, grid: { stroke: GRID, width: 1 }, ticks: { show: false }, size: 24, font: "10px system-ui" },
        { stroke: TICK, grid: { stroke: GRID, width: 1 }, ticks: { show: false }, size: 56, font: "10px system-ui", values: (_u, v) => v.map(fmt) },
      ],
      series: [
        {},
        ...series.map((s) => ({
          label: s.label, stroke: s.color, width: 1.5, points: { show: false },
          fill: s.fill ? s.color + "33" : undefined,
          value: (_u: uPlot, v: number | null) => (v == null ? "—" : fmt(v)),
        })),
      ],
    };
    const u = new uPlot(opts, latest.current as uPlot.AlignedData, el);
    plot.current = u;
    const ro = new ResizeObserver(() => u.setSize({ width: Math.max(el.clientWidth, 50), height: Math.max(el.clientHeight, 50) }));
    ro.observe(el);
    return () => { ro.disconnect(); u.destroy(); plot.current = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [series.length]);

  useEffect(() => { plot.current?.setData(data as uPlot.AlignedData); }, [data]);

  return <div className="mon-chart" ref={box} />;
}
