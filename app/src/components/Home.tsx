import { api, type Sample, type Status, type WifiStatus, type BtStatus } from "../api";
import { budget, duration, gib, percent, profileLabel, stateLabel, watts } from "../format";
import * as I from "../icons";
import { PowerChart } from "./PowerChart";
import { Bar, Button, Card, Pill, Toggle, useAction } from "./ui";
import type { View } from "../App";

export function Home({ status, samples, wifi, bt, go, openTerminal, refresh }: {
  status: Status; samples: Sample[]; wifi: WifiStatus | null; bt: BtStatus | null; go: (v: View) => void;
  openTerminal: (what: "claude" | "shell") => void; refresh: () => void;
}) {
  const p = status.power;
  const { busy, run } = useAction();
  const gpu = status.gpus[0];
  const cap = p.capacity_percent ?? 0;
  const charging = p.state === "charging";
  const runtime = duration(p.runtime_hours);
  const toFull = duration(p.time_to_full_hours);
  const b = p.state === "discharging" ? budget(p.smooth_w) : null;
  const mem = status.memory;
  const btAudio = bt?.devices.find((d) => d.connected && d.icon?.startsWith("audio"));

  const launch = (app: string, label: string) => run(`launch-${app}`, () => api.launch(app), `${label} opened`);

  const tiles: { key: string; label: string; sub: string; icon: React.ReactNode; on: () => void; accent?: boolean; subTone?: string }[] = [
    { key: "claude", label: "Terminal", sub: "Claude Code", icon: <I.Terminal size={34} />, on: () => openTerminal("claude"), accent: true },
    { key: "chrome", label: "Chrome", sub: "Browse · Search", icon: <I.Globe size={34} className="text-cyan" />, on: () => launch("chrome", "Chrome") },
    { key: "wifi", label: "Wi-Fi", sub: wifi?.ssid ? "Connected" : wifi?.enabled === false ? "Off" : "Not connected", icon: <I.Wifi size={34} className="text-cyan" />, on: () => go("wifi"), subTone: wifi?.ssid ? "text-green" : undefined },
    { key: "bt", label: "Bluetooth", sub: btAudio ? btAudio.name : bt?.powered ? "On" : "Off", icon: <I.Bluetooth size={34} className="text-purple" />, on: () => go("bluetooth") },
    { key: "music", label: "Music", sub: "Play in Chrome", icon: <I.Music size={34} className="text-purple" />, on: () => launch("music", "Music") },
    { key: "files", label: "Files", sub: "Browse files", icon: <I.Folder size={34} className="text-muted" />, on: () => launch("files", "Files") },
    { key: "settings", label: "Settings", sub: "System & Power", icon: <I.Settings size={34} className="text-muted" />, on: () => go("settings") },
  ];

  const powerSaver = status.profile.active === "power-saver";

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-wrap items-center gap-5">
        <img src="/logo.svg" alt="" className="h-20 w-20 rounded-2xl" />
        <div>
          <h1 className="text-4xl font-bold tracking-tight">ADG POWER OS</h1>
          <p className="text-lg text-muted">
            Ubuntu · {status.eco_boot ? "Eco boot" : status.eco_applied ? "Low Power Mode" : profileLabel(status.profile.active)}
          </p>
        </div>
        <div className="ml-auto">
          {status.eco_applied ? (
            <button onClick={() => run("revert", api.ecoRevert, "Eco profile reverted", refresh)}
              className="tile flex items-center gap-3 rounded-2xl border border-green/50 bg-green/10 px-5 py-3 text-lg font-semibold text-green">
              <I.Leaf /> {busy === "revert" ? "Reverting…" : "Low Power Active"}
              <span className="text-xs font-normal text-muted">tap to revert</span>
            </button>
          ) : (
            <button onClick={() => run("eco", api.ecoApply, "Eco profile applied", refresh)}
              className="tile flex items-center gap-3 rounded-2xl border border-border bg-panel/80 px-5 py-3 text-lg font-semibold">
              <I.Leaf className="text-green" /> {busy === "eco" ? "Applying…" : "Enable Low Power"}
            </button>
          )}
        </div>
      </header>

      {!status.eco_boot && (
        <div className="card flex flex-wrap items-center gap-4 border-green/40 px-5 py-4">
          <I.Leaf className="text-green" />
          <div className="min-w-0 flex-1">
            <div className="font-semibold">You're on a normal Ubuntu boot</div>
            <div className="text-sm text-muted">Docker, PostgreSQL, Ollama and the NVIDIA services are still running and keep the CPU and RTX busy. Eco mode boots without them, once; the next restart is normal again.</div>
          </div>
          <Button tone="primary" busy={busy === "ecoboot"} onClick={() => {
            if (confirm("Restart into Eco mode now? Save your work first.")) run("ecoboot", () => api.session("eco-reboot"));
          }}>Restart into Eco mode</Button>
          <Button small onClick={() => go("apps")}>See what's using power</Button>
        </div>
      )}

      <div className="grid gap-5 lg:grid-cols-3">
        <Card>
          <div className="mb-3 flex items-center gap-3">
            <I.Battery size={30} level={cap} charging={charging} className={cap < 20 ? "text-red" : "text-green"} />
            <h2 className="text-[17px] font-semibold">Battery</h2>
            <span className="ml-auto text-sm text-muted">{stateLabel(p.state)}</span>
          </div>
          <div className="flex items-end justify-between gap-3">
            <div className="num text-5xl font-bold">{percent(p.capacity_percent)}</div>
            <div className="text-right">
              {runtime ? (
                <><div className="num text-2xl font-semibold">{runtime}</div><div className="text-sm text-muted">estimated left</div></>
              ) : toFull ? (
                <><div className="num text-2xl font-semibold">{toFull}</div><div className="text-sm text-muted">until full (est.)</div></>
              ) : (
                <div className="text-sm text-muted">{p.state === "discharging" ? "Waiting for steady draw" : "No runtime estimate on AC"}</div>
              )}
            </div>
          </div>
          <div className="mt-4"><Bar value={cap} tone={cap < 20 ? "amber" : "green"} label="Battery charge" /></div>
          {p.health_percent != null && (
            <p className="mt-3 text-xs text-muted">Health {p.health_percent.toFixed(0)}% of design · {p.energy_now_wh?.toFixed(1)} Wh now{p.estimated && " · derived from charge × voltage"}</p>
          )}
        </Card>

        <Card>
          <div className="flex gap-4">
            <div className="flex-1">
              <div className="mb-2 flex items-center gap-3"><I.Bolt size={26} className="text-cyan" /><h2 className="text-[17px] font-semibold">Power Draw</h2></div>
              <div className="num text-5xl font-bold">{p.state === "discharging" ? watts(p.smooth_w) : "—"}</div>
              <div className="mt-1 text-xs text-muted">
                {p.state === "discharging" ? (b ? <Pill tone={b.tone}>{b.name}</Pill> : "smoothed") : p.state === "charging" ? `On AC, charging at ${watts(p.smooth_w)}. Unplug to measure usage.` : "On AC: battery draw not measurable"}
              </div>
            </div>
            <dl className="num grid grid-cols-[auto_auto] gap-x-4 gap-y-1 self-start text-sm">
              <dt className="text-muted">Min</dt><dd className="text-right">{watts(p.window_5m.min_w)}</dd>
              <dt className="text-muted">Avg</dt><dd className="text-right">{watts(p.window_5m.avg_w)}</dd>
              <dt className="text-muted">Max</dt><dd className="text-right">{watts(p.window_5m.max_w)}</dd>
            </dl>
          </div>
          <div className="mt-3"><PowerChart samples={samples.slice(-100)} height={56} compact /></div>
          <p className="mt-1 text-[11px] text-muted">5-minute window · readings are estimates</p>
        </Card>

        <Card title="System" icon={<I.Chip size={26} className="text-purple" />}>
          <div className="grid grid-cols-[3.5rem_1fr_auto] items-center gap-x-4 gap-y-4 text-sm">
            <span>CPU</span><Bar value={status.cpu_percent ?? 0} tone="purple" label="CPU" /><span className="num text-right">{percent(status.cpu_percent)}</span>
            <span>RAM</span><Bar value={mem ? (mem.used_bytes / mem.total_bytes) * 100 : 0} tone="green" label="Memory" />
            <span className="num text-right">{mem ? `${gib(mem.used_bytes)} / ${gib(mem.total_bytes)} GB` : "—"}</span>
          </div>
          {gpu && (
            <div className="mt-4 flex items-center justify-between rounded-xl bg-panel-hover/50 px-3 py-2 text-sm">
              <span>GPU <span className="text-muted">({gpu.vendor === "NVIDIA" ? "RTX 5060" : gpu.name})</span></span>
              {gpu.suspended ? <Pill tone="green">Suspended</Pill> : <Pill tone="amber">Active · {gpu.runtime_status}</Pill>}
            </div>
          )}
          <p className="mt-2 text-[11px] text-muted">GPU state read from the kernel's runtime PM, not a toggle.</p>
        </Card>
      </div>

      <nav className="grid grid-cols-2 gap-4 sm:grid-cols-4 xl:grid-cols-7" aria-label="Launcher">
        {tiles.map((t) => (
          <button key={t.key} onClick={t.on} disabled={busy === `launch-${t.key}`}
            className={`tile card flex flex-col items-center gap-2 px-3 py-5 ${t.accent ? "!border-purple !bg-purple/35" : ""}`}>
            {t.icon}
            <span className="text-lg font-semibold">{t.label}</span>
            <span className={`max-w-full truncate text-sm ${t.subTone ?? "text-muted"}`}>{t.sub}</span>
          </button>
        ))}
      </nav>

      <div className="grid gap-5 lg:grid-cols-[1.5fr_1fr]">
        <Card title="Power history" icon={<I.Bolt size={20} />} right={<span className="text-xs text-muted">last 30 min · 3 s samples</span>}>
          <PowerChart samples={samples} height={200} />
        </Card>
        <Card title="Quick Actions" icon={<I.Bolt size={22} />}>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex items-center gap-3 rounded-xl border border-border bg-panel-hover/40 p-3">
              <I.Leaf className="text-green" />
              <div className="flex-1"><div className="font-medium">Power Saver</div><div className={`text-sm ${powerSaver ? "text-green" : "text-muted"}`}>{profileLabel(status.profile.active)}</div></div>
              <Toggle label="Power saver" on={powerSaver} disabled={!status.profile.available}
                onChange={(on) => run("profile", () => api.setProfile(on ? "power-saver" : "balanced"), on ? "Power saver on" : "Balanced profile", refresh)} />
            </div>
            <div className="flex items-center gap-3 rounded-xl border border-border bg-panel-hover/40 p-3">
              <I.Gpu className="text-green" />
              <div className="flex-1">
                <div className="font-medium">RTX 5060</div>
                <div className="text-sm text-muted">{gpu ? (gpu.suspended ? "Asleep · kernel-verified" : "Awake, drawing power") : "Not found"}</div>
              </div>
              {gpu && (gpu.suspended
                ? <Pill tone="green">Off</Pill>
                : gpu.runtime_control !== "auto"
                  ? <Button small onClick={() => run("gpu", () => api.helper("gpu-runtime-pm", gpu.pci_address, "auto"), "GPU may now suspend when idle", refresh)}>Allow sleep</Button>
                  : <button className="text-xs text-amber underline" onClick={() => go("apps")}>Why awake?</button>)}
            </div>
            <div className="flex items-center gap-3 rounded-xl border border-border bg-panel-hover/40 p-3">
              <I.Monitor className="text-muted" />
              <div className="flex-1"><div className="font-medium">Display</div><div className="text-sm text-muted">{status.sway ? "60 Hz preferred in eco" : "Refresh set by session"} · {percent(status.backlight?.percent)}</div></div>
              <button className="text-muted" onClick={() => go("settings")} aria-label="Display settings"><I.Chevron className="-rotate-90" /></button>
            </div>
            <div className="rounded-xl border border-border bg-panel-hover/40 p-3">
              <div className="flex items-center gap-3"><I.Sun className="text-muted" /><span className="flex-1 font-medium">Brightness</span><span className="num text-sm">{percent(status.backlight?.percent)}</span></div>
              <input type="range" min={5} max={100} step={5} aria-label="Brightness" disabled={!status.backlight}
                defaultValue={Math.round(status.backlight?.percent ?? 50)} key={status.backlight ? "bl" : "none"}
                onPointerUp={(e) => run("bright", () => api.setBrightness(Number(e.currentTarget.value)), undefined, refresh)}
                onKeyUp={(e) => run("bright", () => api.setBrightness(Number(e.currentTarget.value)), undefined, refresh)}
                className="mt-2 w-full" />
            </div>
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            {(["power-saver", "balanced", "performance"] as const).map((pr) => (
              <Button key={pr} small tone={status.profile.active === pr ? "primary" : "default"} disabled={!status.profile.choices.includes(pr)}
                onClick={() => run("profile", () => api.setProfile(pr), `${profileLabel(pr)} profile`, refresh)}>{profileLabel(pr)}</Button>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
