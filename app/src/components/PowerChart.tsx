import { useMemo, useRef, useState } from "react";
import type { Sample } from "../api";

/**
 * Single-series line of battery draw over time, with a crosshair tooltip.
 * Raw readings are drawn faint; the smoothed estimate is the main line.
 */
export function PowerChart({ samples, height = 180, compact = false }: { samples: Sample[]; height?: number; compact?: boolean }) {
  const ref = useRef<SVGSVGElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const W = 600;
  const H = height;
  const pad = compact ? { l: 0, r: 0, t: 6, b: 4 } : { l: 40, r: 8, t: 10, b: 22 };

  // Only on-battery readings are power draw. On AC the gauge reports charge
  // current into the battery, which looks like a 30 W "draw" and misleads.
  const pts = useMemo(() => samples.filter((s) => s.smooth_w != null && s.discharging), [samples]);
  const view = useMemo(() => {
    if (pts.length < 2) return null;
    const t0 = pts[0].t;
    const t1 = pts[pts.length - 1].t;
    const vals = pts.flatMap((p) => [p.w ?? p.smooth_w!, p.smooth_w!]);
    const maxW = Math.max(10, Math.ceil(Math.max(...vals) / 10) * 10);
    const x = (t: number) => pad.l + ((t - t0) / Math.max(1, t1 - t0)) * (W - pad.l - pad.r);
    const y = (w: number) => pad.t + (1 - w / maxW) * (H - pad.t - pad.b);
    const line = (get: (s: Sample) => number | null) =>
      pts.map((p, i) => `${i ? "L" : "M"}${x(p.t).toFixed(1)},${y(get(p) ?? 0).toFixed(1)}`).join("");
    const smooth = line((s) => s.smooth_w);
    const raw = line((s) => s.w);
    const area = `${smooth}L${x(t1)},${H - pad.b}L${x(t0)},${H - pad.b}Z`;
    const ticks = [0, maxW / 2, maxW];
    return { x, y, smooth, raw, area, maxW, ticks, t0, t1 };
  }, [pts, H, pad.l, pad.r, pad.t, pad.b]);

  if (!view) {
    return (
      <div className="grid place-items-center px-4 text-center text-sm text-muted" style={{ height }}>
        {samples.length > 0 && !samples.some((s) => s.discharging)
          ? (compact ? "Unplug to measure draw" : "On AC. Unplug to see battery draw; while charging the sensor reports charge rate, not usage.")
          : "Collecting readings…"}
      </div>
    );
  }

  const onMove = (e: React.PointerEvent) => {
    const r = ref.current!.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W;
    let best = 0;
    let bd = Infinity;
    pts.forEach((p, i) => {
      const d = Math.abs(view.x(p.t) - px);
      if (d < bd) { bd = d; best = i; }
    });
    setHover(best);
  };

  const hp = hover != null ? pts[hover] : null;
  const time = (t: number) => new Date(t * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div className="relative">
      <svg ref={ref} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="block w-full touch-none" style={{ height }}
        onPointerMove={onMove} onPointerLeave={() => setHover(null)} role="img"
        aria-label={`Battery draw over the last ${Math.round((view.t1 - view.t0) / 60)} minutes`}>
        <defs>
          <linearGradient id="pc-fill" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0" stopColor="var(--color-cyan)" stopOpacity="0.28" />
            <stop offset="1" stopColor="var(--color-cyan)" stopOpacity="0" />
          </linearGradient>
        </defs>
        {!compact && view.ticks.map((t) => (
          <g key={t}>
            <line x1={pad.l} x2={W - pad.r} y1={view.y(t)} y2={view.y(t)} stroke="var(--color-border)" strokeWidth="1" strokeDasharray={t === 0 ? undefined : "3 5"} vectorEffect="non-scaling-stroke" />
            <text x={pad.l - 6} y={view.y(t) + 4} textAnchor="end" fontSize="11" fill="var(--color-muted)">{t}W</text>
          </g>
        ))}
        <path d={view.area} fill="url(#pc-fill)" />
        {!compact && <path d={view.raw} fill="none" stroke="var(--color-cyan)" strokeOpacity="0.25" strokeWidth="1" vectorEffect="non-scaling-stroke" />}
        <path d={view.smooth} fill="none" stroke={compact ? "var(--color-green)" : "var(--color-cyan)"} strokeWidth="2" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
        {hp && (
          <g>
            <line x1={view.x(hp.t)} x2={view.x(hp.t)} y1={pad.t} y2={H - pad.b} stroke="var(--color-muted)" strokeWidth="1" vectorEffect="non-scaling-stroke" />
            <circle cx={view.x(hp.t)} cy={view.y(hp.smooth_w!)} r="4" fill="var(--color-cyan)" stroke="var(--color-panel)" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          </g>
        )}
        {!compact && (
          <>
            <text x={pad.l} y={H - 4} fontSize="11" fill="var(--color-muted)">{time(view.t0)}</text>
            <text x={W - pad.r} y={H - 4} fontSize="11" textAnchor="end" fill="var(--color-muted)">{time(view.t1)}</text>
          </>
        )}
      </svg>
      {hp && !compact && (
        <div className="pointer-events-none absolute top-1 rounded-lg border border-border bg-bg/95 px-3 py-2 text-xs shadow-lg"
          style={{ left: `clamp(0px, calc(${(view.x(hp.t) / W) * 100}% - 70px), calc(100% - 150px))` }}>
          <div className="text-muted">{time(hp.t)}</div>
          <div className="num font-semibold">{hp.smooth_w!.toFixed(2)} W smoothed</div>
          {hp.w != null && <div className="num text-muted">{hp.w.toFixed(2)} W raw</div>}
          <div className="text-muted">{hp.discharging ? "On battery" : "On AC"}</div>
        </div>
      )}
    </div>
  );
}
