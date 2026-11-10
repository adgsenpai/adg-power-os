// Talks to adg-powerd. In Tauri the request goes through the `daemon` command
// (Rust forwards it to the Unix socket); in a browser it goes to /api via the
// Vite proxy. Passwords are only ever sent in request bodies.

export type ChargeState = "discharging" | "charging" | "full" | "ac_idle" | "unknown";

export interface Battery {
  name: string;
  status_raw: string;
  capacity_percent: number | null;
  power_w: number | null;
  power_source: "direct" | "derived" | "missing";
  energy_now_wh: number | null;
  energy_full_wh: number | null;
  energy_full_design_wh: number | null;
  voltage_v: number | null;
  cycle_count: number | null;
}

export interface Power {
  batteries: Battery[];
  ac_online: boolean | null;
  state: ChargeState;
  capacity_percent: number | null;
  power_w: number | null;
  smooth_w: number | null;
  energy_now_wh: number | null;
  energy_full_wh: number | null;
  estimated: boolean;
  runtime_hours: number | null;
  time_to_full_hours: number | null;
  health_percent: number | null;
  window_5m: { min_w: number | null; avg_w: number | null; max_w: number | null; samples: number };
}

export interface Gpu {
  pci_address: string;
  name: string;
  vendor: string;
  driver: string | null;
  runtime_status: string;
  runtime_control: string | null;
  suspended: boolean;
  nvidia_video_memory: string | null;
}

export interface Status {
  time: number;
  hostname: string;
  power: Power;
  cpu_percent: number | null;
  memory: { total_bytes: number; used_bytes: number } | null;
  gpus: Gpu[];
  backlight: { device: string; brightness: number; max_brightness: number; percent: number } | null;
  keyboard_backlight: [number, number] | null;
  profile: { available: boolean; active: string | null; choices: string[]; degraded: string | null };
  eco_applied: boolean;
  eco_boot: boolean;
  sway: boolean;
}

export interface Sample { t: number; w: number | null; smooth_w: number | null; capacity: number | null; discharging: boolean }

export interface WifiStatus {
  available: boolean; enabled: boolean; interface: string | null; ssid: string | null;
  strength: number | null; ipv4: string | null; state: string; error: string | null;
}
export interface AccessPoint { ssid: string; bssid: string; strength: number; frequency_mhz: number; security: string; active: boolean; known: boolean }
export interface BtDevice { address: string; name: string; icon: string | null; paired: boolean; trusted: boolean; connected: boolean; rssi: number | null; battery_percent: number | null }
export interface BtStatus { available: boolean; powered: boolean; discovering: boolean; adapter: string | null; devices: BtDevice[]; error: string | null }
export interface Sink { id: number; name: string; volume: number | null; muted: boolean; default: boolean; bluetooth: boolean }
export interface AudioStatus { sinks: Sink[]; playing: { player: string; status: string; title: string | null; artist: string | null }[]; error: string | null }
export interface ServiceRow {
  unit: string; scope: "system" | "user"; category: string; eco: "blocked_at_eco_boot" | "on_demand" | "keep";
  installed: boolean; active_state: string | null; sub_state: string | null; unit_file_state: string | null;
  description: string | null; stoppable: boolean; stop_blocker: string | null;
}
export interface AuditEntry { t: number; action: string; detail: string; ok: boolean }
export interface RunningApp { unit: string; app: string; memory_bytes: number | null; cpu_seconds: number | null; tasks: number | null; started: number | null; windows: number | null }
export interface Hogs { top: { pid: number; name: string; cpu_percent: number; rss_bytes: number }[]; gpu_users: { pid: number; name: string }[]; gpu_services: string[] }
export type SessionAction = "lock" | "logout" | "suspend" | "reboot" | "poweroff" | "eco-reboot";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Every request gives up after this, so a stuck call can never freeze the UI. */
const TIMEOUT_MS = 15000;

function withTimeout<T>(p: Promise<T>, ms = TIMEOUT_MS): Promise<T> {
  return new Promise((resolve, reject) => {
    const t = window.setTimeout(() => reject(new Error("adg-powerd did not answer in time")), ms);
    p.then((v) => { window.clearTimeout(t); resolve(v); }, (e) => { window.clearTimeout(t); reject(e); });
  });
}

interface Envelope<T> { ok: boolean; data?: T; error?: string }

export async function call<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
  let env: Envelope<T>;
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    env = await withTimeout(invoke<Envelope<T>>("daemon", { method, path, body: body ?? null }));
  } else {
    const ctl = new AbortController();
    const t = window.setTimeout(() => ctl.abort(), TIMEOUT_MS);
    const res = await fetch(path, {
      method,
      signal: ctl.signal,
      headers: body ? { "Content-Type": "application/json" } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    }).finally(() => window.clearTimeout(t));
    if (res.status === 502 || res.status === 504 || res.status === 500) throw new Error("adg-powerd is not running");
    env = await res.json();
  }
  if (!env.ok) throw new Error(env.error ?? "Request failed");
  return env.data as T;
}

export const api = {
  status: () => call<Status>("GET", "/api/status"),
  history: (minutes = 30) => call<{ interval_s: number; samples: Sample[] }>("GET", `/api/history?minutes=${minutes}`),
  setProfile: (name: string) => call("POST", "/api/profile", { name }),
  setBrightness: (percent: number) => call("POST", "/api/brightness", { percent }),
  setKeyboard: (value: number) => call("POST", "/api/keyboard", { value }),
  ecoApply: () => call<{ steps: string[] }>("POST", "/api/eco/apply"),
  ecoRevert: () => call<{ steps: string[] }>("POST", "/api/eco/revert"),
  helper: (action: string, target: string, value: string) => call("POST", "/api/helper", { action, target, value }),
  wifi: () => call<WifiStatus>("GET", "/api/wifi"),
  networks: () => call<AccessPoint[]>("GET", "/api/wifi/networks"),
  scan: () => call("POST", "/api/wifi/scan"),
  wifiEnabled: (on: boolean) => call("POST", "/api/wifi/enabled", { on }),
  connect: (ssid: string, password?: string) => call("POST", "/api/wifi/connect", { ssid, password }),
  disconnect: () => call("POST", "/api/wifi/disconnect"),
  bluetooth: () => call<BtStatus>("GET", "/api/bluetooth"),
  bt: (action: string, address?: string, on?: boolean) => call("POST", `/api/bluetooth/${action}`, { address, on }),
  audio: () => call<AudioStatus>("GET", "/api/audio"),
  setSink: (id: number) => call("POST", "/api/audio/default", { id }),
  setVolume: (id: number, volume: number) => call("POST", "/api/audio/volume", { id, volume }),
  playPause: () => call("POST", "/api/audio/playpause"),
  services: () => call<{ services: ServiceRow[]; running_containers: number }>("GET", "/api/services"),
  serviceStop: (unit: string) => call("POST", "/api/services/stop", { unit }),
  serviceStart: (unit: string) => call("POST", "/api/services/start", { unit }),
  launch: (app: string) => call("POST", "/api/launch", { app }),
  audit: () => call<AuditEntry[]>("GET", "/api/audit"),
  apps: () => call<{ apps: RunningApp[]; sway: boolean }>("GET", "/api/apps"),
  closeApp: (unit: string) => call("POST", "/api/apps/close", { unit }),
  hogs: () => call<Hogs>("GET", "/api/hogs"),
  session: (a: SessionAction) => call("POST", `/api/session/${a}`),
};
