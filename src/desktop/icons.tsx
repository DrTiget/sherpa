import type { IconName } from "./types";

const P = { fill: "none", stroke: "currentColor", strokeWidth: 1.7, strokeLinecap: "round", strokeLinejoin: "round" } as const;

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      {name === "servers" && <g {...P}><rect x="3" y="4" width="18" height="6" rx="1.5" /><rect x="3" y="14" width="18" height="6" rx="1.5" /><path d="M7 7h.01M7 17h.01" /></g>}
      {name === "terminal" && <g {...P}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="m7 9 3 3-3 3M13 15h4" /></g>}
      {name === "run" && <g {...P}><circle cx="12" cy="12" r="9" /><path d="m10 8.5 5 3.5-5 3.5z" /></g>}
      {name === "monitor" && <g {...P}><path d="M3 12h4l3-7 4 14 3-7h4" /></g>}
      {name === "app" && <g {...P}><rect x="4" y="4" width="16" height="16" rx="3" /></g>}
    </svg>
  );
}
