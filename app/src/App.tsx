import { useEffect, useState } from "react";
import { api } from "./api";
import { duration, percent, profileLabel, bars } from "./format";
import * as I from "./icons";
import { Home } from "./components/Home";
import { AudioView, BluetoothView, WifiView } from "./components/Connectivity";
import { ServicesView, SettingsView } from "./components/System";
import { ToastHost, usePoll } from "./components/ui";

export type View = "home" | "wifi" | "bluetooth" | "audio" | "services" | "settings";

const dock: { view: View; label: string; icon: React.ReactNode }[] = [
  { view: "home", label: "Dashboard", icon: <I.Grid /> },
  { view: "wifi", label: "Wi-Fi", icon: <I.Wifi className="text-cyan" /> },
  { view: "bluetooth", label: "Bluetooth", icon: <I.Bluetooth className="text-cyan" /> },
  { view: "audio", label: "Audio", icon: <I.Music className="text-purple" /> },
  { view: "services", label: "Services", icon: <I.Shield /> },
  { view: "settings", label: "Settings", icon: <I.Settings /> },
];

function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = window.setInterval(() => setNow(new Date()), 15000);
    return () => window.clearInterval(t);
  }, []);
  return (
    <span className="num whitespace-nowrap">
      {now.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}{"  "}
      {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
    </span>
  );
}

function Shell() {
  // ?view=services etc. opens a section directly (handy for links and screenshots).
  const [view, setView] = useState<View>(() => {
    const v = new URLSearchParams(window.location.search).get("view");
    return dock.some((d) => d.view === v) ? (v as View) : "home";
  });
  const status = usePoll(api.status, 3000);
  const history = usePoll(() => api.history(30), 6000);
  const wifi = usePoll(api.wifi, 10000);
  const bt = usePoll(api.bluetooth, 10000);
  const s = status.data;
  const p = s?.power;
  const btAudio = bt.data?.devices.some((d) => d.connected && d.icon?.startsWith("audio"));

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-14 shrink-0 items-center gap-4 border-b border-border/60 bg-bg/70 px-5 backdrop-blur-md">
        <img src="/logo.svg" alt="" className="h-8 w-8 rounded-lg" />
        <span className="font-semibold tracking-wide">ADG POWER OS</span>
        {s && (
          <button onClick={() => setView("settings")} className="mx-auto hidden items-center gap-2 rounded-full border border-border bg-panel/80 px-4 py-1.5 text-sm md:flex">
            <I.Leaf size={18} className={s.profile.active === "power-saver" ? "text-green" : "text-muted"} />
            {s.eco_applied ? "Low Power Mode" : profileLabel(s.profile.active)}
            <I.Chevron size={16} className="text-muted" />
          </button>
        )}
        <div className="ml-auto flex items-center gap-4 text-sm md:ml-0">
          <button aria-label="Wi-Fi" onClick={() => setView("wifi")}>
            {wifi.data?.ssid ? <I.Wifi level={bars(wifi.data.strength)} size={20} /> : <I.WifiOff size={20} className="text-muted" />}
          </button>
          <button aria-label="Bluetooth" onClick={() => setView("bluetooth")} className={bt.data?.powered ? "" : "text-muted"}><I.Bluetooth size={20} /></button>
          <button aria-label="Audio" onClick={() => setView("audio")} className={btAudio ? "text-cyan" : ""}><I.Speaker size={20} /></button>
          {p && (
            <span className="flex items-center gap-2">
              <I.Battery size={24} level={p.capacity_percent ?? 0} charging={p.state === "charging"} className={(p.capacity_percent ?? 100) < 20 ? "text-red" : "text-green"} />
              <span className="num">{percent(p.capacity_percent)}{duration(p.runtime_hours) && ` · ${duration(p.runtime_hours)}`}</span>
            </span>
          )}
          <Clock />
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <nav className="flex w-[76px] shrink-0 flex-col items-center gap-2 border-r border-border/60 bg-bg/60 py-4 backdrop-blur-md" aria-label="Sections">
          {dock.map((d) => (
            <button key={d.view} onClick={() => setView(d.view)} aria-label={d.label} title={d.label}
              aria-current={view === d.view ? "page" : undefined}
              className={`grid h-12 w-12 place-items-center rounded-2xl transition ${view === d.view ? "bg-purple/30 ring-1 ring-purple/60" : "hover:bg-panel-hover"}`}>
              {d.icon}
            </button>
          ))}
          <button onClick={() => api.launch("claude")} aria-label="Claude Code" title="Claude Code in terminal"
            className="mt-auto grid h-12 w-12 place-items-center rounded-2xl hover:bg-panel-hover"><I.Terminal /></button>
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
            <Home status={s} samples={history.data?.samples ?? []} wifi={wifi.data} bt={bt.data} go={setView} refresh={status.refresh} />
          )}
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
