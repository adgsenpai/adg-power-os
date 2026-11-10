import type { ChargeState } from "./api";

export function duration(hours: number | null | undefined): string | null {
  if (hours == null || !Number.isFinite(hours) || hours <= 0) return null;
  const total = Math.round(hours * 60);
  if (total > 99 * 60) return ">99h";
  const h = Math.floor(total / 60);
  const m = total % 60;
  return h > 0 ? `${h}h ${m.toString().padStart(2, "0")}m` : `${m}m`;
}

export function watts(w: number | null | undefined, digits = 1): string {
  return w == null || !Number.isFinite(w) ? "—" : `${w.toFixed(digits)} W`;
}

export function gib(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)}`;
}

export function percent(p: number | null | undefined): string {
  return p == null ? "—" : `${Math.round(p)}%`;
}

export function stateLabel(s: ChargeState): string {
  return {
    discharging: "On battery",
    charging: "Charging",
    full: "Full · on AC",
    ac_idle: "On AC · not charging",
    unknown: "Status unknown",
  }[s];
}

/** Signal bars 0–4 from NM strength 0–100. */
export function bars(strength: number | null | undefined): number {
  if (strength == null) return 0;
  return strength >= 75 ? 4 : strength >= 50 ? 3 : strength >= 25 ? 2 : strength > 0 ? 1 : 0;
}

export function profileLabel(p: string | null | undefined): string {
  return p === "power-saver" ? "Power Saver" : p === "balanced" ? "Balanced" : p === "performance" ? "Performance" : "Unknown";
}

/** Which spec budget a draw falls in. */
export function budget(w: number | null | undefined): { name: string; tone: "green" | "cyan" | "amber" | "red" } | null {
  if (w == null) return null;
  if (w < 8) return { name: "Deep Eco range", tone: "green" };
  if (w < 12) return { name: "Developer range", tone: "cyan" };
  if (w < 15) return { name: "Connected range", tone: "amber" };
  return { name: "Performance range", tone: "red" };
}
