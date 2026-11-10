import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { api, type SessionAction } from "./api";
import { duration, percent, profileLabel, bars } from "./format";
import * as I from "./icons";
import { Home } from "./components/Home";
import { AudioView, BluetoothView, WifiView } from "./components/Connectivity";
import { ServicesView, SettingsView } from "./components/System";
// xterm.js is only loaded when the terminal is first opened.
const TerminalView = lazy(() => import("./components/Terminal").then((m) => ({ default: m.TerminalView })));
import { AppsView } from "./components/Apps";
import { ToastHost, useAction, usePoll } from "./components/ui";

export type View = "home" | "terminal" | "apps" | "wifi" | "bluetooth" | "audio" | "services" | "settings";

const dock: { view: View; label: string; icon: React.ReactNode }[] = [
  { view: "home", label: "Dashboard", icon: <I.Grid /> },
  { view: "terminal", label: "Terminal", icon: <I.Terminal className="text-purple" /> },
  { view: "apps", label: "Running apps & power use", icon: <I.Bolt className="text-amber" /> },
  { view: "wifi", label: "Wi-Fi", icon: <I.Wifi className="text-cyan" /> },
  { view: "bluetooth", label: "Bluetooth", icon: <I.Bluetooth className="text-cyan" /> },
  { view: "audio", label: "Audio", icon: <I.Music className="text-purple" /> },
  { view: "services", label: "Services", icon: <I.Shield /> },
  { view: "settings", label: "Settings", icon: <I.Settings /> },
];

function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    // Minute resolution: no reason to repaint more often.
    const t = window.setInterval(() => setNow(new Date()), 30000);
    return () => window.clearInterval(t);
  }, []);
  return (
    <span className="num whitespace-nowrap">
      {now.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}{"  "}
      {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
    </span>
  );
}

const sessionItems: { action: SessionAction; label: string; icon: React.ReactNode; confirm?: string; tone?: string }[] = [
  { action: "lock", label: "Lock", icon: <I.Lock size={18} /> },
  { action: "logout", label: "Log out", icon: <I.LogOut size={18} />, confirm: "Log out of this session? Unsaved work in open apps may be lost." },
  { action: "suspend", label: "Suspend", icon: <I.Moon size={18} /> },
  { action: "eco-reboot", label: "Restart into Eco mode", icon: <I.Leaf size={18} />, tone: "text-green", confirm: "Restart into ADG Power OS Eco mode? Docker, databases, Ollama and the NVIDIA services stay off for that boot. The next restart is normal Ubuntu again." },
  { action: "reboot", label: "Restart", icon: <I.Restart size={18} />, confirm: "Restart now?" },
  { action: "poweroff", label: "Shut down", icon: <I.Power size={18} />, tone: "text-red", confirm: "Shut down now?" },
];

function PowerMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { run } = useAction();
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", esc); };
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <button aria-label="Power menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}
        className="grid h-9 w-9 place-items-center rounded-full hover:bg-panel-hover"><I.Power size={19} /></button>
      {open && (
        <div role="menu" className="card absolute right-0 top-11 z-50 w-64 p-1.5 shadow-2xl">
          {sessionItems.map((it) => (
            <button key={it.action} role="menuitem"
              onClick={() => {
                if (it.confirm && !confirm(it.confirm)) return;
                setOpen(false);
                run(it.action, () => api.session(it.action));
              }}
              className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm hover:bg-panel-hover ${it.tone ?? ""}`}>
              {it.icon}{it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function Shell() {
  const [view, setView] = useState<View>(() => {
    const v = new URLSearchParams(window.location.search).get("view");
    return dock.some((d) => d.view === v) ? (v as View) : "home";
  });
  const [termRequest, setTermRequest] = useState<{ what: "claude" | "shell"; n: number } | null>(null);
  // Mount the terminal once and keep it alive while switching views, so tabs survive.
  const [termMounted, setTermMounted] = useState(view === "terminal");
  const status = usePoll(api.status, 5000);
  const history = usePoll(() => api.history(30), view === "home" ? 15000 : 60000, [view]);
  const wifi = usePoll(api.wifi, 20000);
  const bt = usePoll(api.bluetooth, 20000);
  const s = status.data;
  const p = s?.power;
  const btAudio = bt.data?.devices.some((d) => d.connected && d.icon?.startsWith("audio"));

  const go = (v: View) => {
    if (v === "terminal") setTermMounted(true);
    setView(v);
  };
  const openTerminal = (what: "claude" | "shell") => {
    setTermRequest((r) => ({ what, n: (r?.n ?? 0) + 1 }));
    go("terminal");
  };

  return (
    <div className="flex h-full flex-col">
      <div className="wallpaper" />
      <div className="flex h-14 shrink-0 items-center gap-4 border-b border-border/60 bg-bg/90 px-5">
        <img src="/logo.svg" alt="" className="h-8 w-8 rounded-lg" />
        <span className="font-semibold tracking-wide">ADG POWER OS</span>
        {s && (
          <button onClick={() => go("settings")} className="mx-auto hidden items-center gap-2 rounded-full border border-border bg-panel px-4 py-1.5 text-sm md:flex">
            <I.Leaf size={18} className={s.profile.active === "power-saver" ? "text-green" : "text-muted"} />
            {s.eco_boot ? "Eco boot" : s.eco_applied ? "Low Power Mode" : profileLabel(s.profile.active)}
            <I.Chevron size={16} className="text-muted" />
          </button>
        )}
        <div className="ml-auto flex items-center gap-4 text-sm md:ml-0">
          <button aria-label="Wi-Fi" onClick={() => go("wifi")}>
            {wifi.data?.ssid ? <I.Wifi level={bars(wifi.data.strength)} size={20} /> : <I.WifiOff size={20} className="text-muted" />}
          </button>
          <button aria-label="Bluetooth" onClick={() => go("bluetooth")} className={bt.data?.powered ? "" : "text-muted"}><I.Bluetooth size={20} /></button>
          <button aria-label="Audio" onClick={() => go("audio")} className={btAudio ? "text-cyan" : ""}><I.Speaker size={20} /></button>
          {p && (
            <span className="flex items-center gap-2">
              <I.Battery size={24} level={p.capacity_percent ?? 0} charging={p.state === "charging"} className={(p.capacity_percent ?? 100) < 20 ? "text-red" : "text-green"} />
              <span className="num">{percent(p.capacity_percent)}{duration(p.runtime_hours) && ` · ${duration(p.runtime_hours)}`}</span>
              {p.state === "discharging" && p.smooth_w != null && <span className="num text-muted">{p.smooth_w.toFixed(1)} W</span>}
            </span>
          )}
          <Clock />
          <PowerMenu />
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <nav className="flex w-[76px] shrink-0 flex-col items-center gap-2 border-r border-border/60 bg-bg/90 py-4" aria-label="Sections">
          {dock.map((d) => (
            <button key={d.view} onClick={() => go(d.view)} aria-label={d.label} title={d.label}
              aria-current={view === d.view ? "page" : undefined}
              className={`grid h-12 w-12 place-items-center rounded-2xl ${view === d.view ? "bg-purple/30 ring-1 ring-purple/60" : "hover:bg-panel-hover"}`}>
              {d.icon}
            </button>
          ))}
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto p-6">
          {status.error && !s && (
            <div className="card mx-auto mt-24 max-w-lg p-8 text-center">
              <I.Power size={40} className="mx-auto text-amber" />
              <h1 className="mt-3 text-xl font-semibold">Can't reach adg-powerd</h1>
              <p className="mt-2 text-sm text-muted">{status.error}. Start it with <code className="text-cyan">systemctl --user start adg-powerd</code>. Networking keeps working without it.</p>
            </div>
          )}
          {s && view === "home" && (
            <Home status={s} samples={history.data?.samples ?? []} wifi={wifi.data} bt={bt.data} go={go} openTerminal={openTerminal} refresh={status.refresh} />
          )}
          {termMounted && <div className={view === "terminal" ? "" : "hidden"}><Suspense fallback={<div className="text-sm text-muted">Loading terminal…</div>}><TerminalView request={termRequest} /></Suspense></div>}
          {view === "apps" && <AppsView />}
          {view === "wifi" && <WifiView />}
          {view === "bluetooth" && <BluetoothView />}
          {view === "audio" && <AudioView />}
          {view === "services" && <ServicesView />}
          {s && view === "settings" && <SettingsView status={s} refresh={status.refresh} />}
        </main>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <ToastHost>
      <Shell />
    </ToastHost>
  );
}
