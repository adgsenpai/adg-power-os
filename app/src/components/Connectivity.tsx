import { useState } from "react";
import { api, type AccessPoint } from "../api";
import { bars, percent } from "../format";
import * as I from "../icons";
import { Button, Card, Empty, Pill, Toggle, useAction, usePoll } from "./ui";

export function WifiView() {
  const status = usePoll(api.wifi, 8000);
  const nets = usePoll(api.networks, 15000);
  const { busy, run } = useAction();
  const [pending, setPending] = useState<AccessPoint | null>(null);
  const [password, setPassword] = useState("");
  const s = status.data;
  const refreshAll = () => { status.refresh(); nets.refresh(); };

  const connect = (ap: AccessPoint, pw?: string) =>
    run(`c-${ap.ssid}`, () => api.connect(ap.ssid, pw), `Connecting to ${ap.ssid}`, () => {
      setPending(null);
      setPassword("");
      window.setTimeout(refreshAll, 2500);
    });

  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1.4fr]">
      <Card title="Wi-Fi" icon={<I.Wifi />} right={s && <Toggle label="Wi-Fi radio" on={s.enabled} onChange={(on) => run("radio", () => api.wifiEnabled(on), on ? "Wi-Fi on" : "Wi-Fi off", refreshAll)} />}>
        {status.error && <p className="text-sm text-red">{status.error}</p>}
        {s && (
          <dl className="grid grid-cols-[7rem_1fr] gap-y-2 text-sm">
            <dt className="text-muted">Network</dt><dd className="font-medium">{s.ssid ?? "Not connected"}</dd>
            <dt className="text-muted">State</dt><dd>{s.state}</dd>
            <dt className="text-muted">Signal</dt><dd>{percent(s.strength)}</dd>
            <dt className="text-muted">IP address</dt><dd className="num">{s.ipv4 ?? "—"}</dd>
            <dt className="text-muted">Interface</dt><dd>{s.interface ?? "—"}</dd>
          </dl>
        )}
        <div className="mt-4 flex flex-wrap gap-2">
          <Button small onClick={() => run("scan", api.scan, "Scanning…", () => window.setTimeout(nets.refresh, 3000))} busy={busy === "scan"}>
            <span className="inline-flex items-center gap-2"><I.Refresh size={16} /> Scan</span>
          </Button>
          {s?.ssid && <Button small tone="danger" onClick={() => run("dc", api.disconnect, "Disconnected", refreshAll)} busy={busy === "dc"}>Disconnect</Button>}
          <Button small onClick={() => run("nce", () => api.launch("network-editor"))}>Advanced…</Button>
        </div>
        <p className="mt-4 text-xs text-muted">Managed by NetworkManager, so Wi-Fi keeps working when this dashboard is closed. Passwords go straight to NetworkManager and are never logged.</p>
      </Card>

      <Card title="Networks in range" right={<span className="text-xs text-muted">{nets.data?.length ?? 0} found</span>}>
        {nets.error && <p className="text-sm text-red">{nets.error}</p>}
        {nets.data?.length === 0 && <Empty>No networks yet. Press Scan.</Empty>}
        <ul className="flex flex-col gap-2">
          {nets.data?.map((ap) => (
            <li key={ap.ssid} className="rounded-xl border border-border bg-panel-hover/30 p-3">
              <div className="flex items-center gap-3">
                <I.Wifi level={bars(ap.strength)} className={ap.active ? "text-green" : "text-cyan"} />
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{ap.ssid}</div>
                  <div className="flex items-center gap-2 text-xs text-muted">
                    {ap.security !== "Open" && <I.Lock size={12} />}{ap.security} · {ap.frequency_mhz >= 5000 ? "5 GHz" : "2.4 GHz"} · {ap.strength}%
                  </div>
                </div>
                {ap.active ? <Pill tone="green">Connected</Pill> : (
                  <Button small tone="primary" busy={busy === `c-${ap.ssid}`}
                    onClick={() => (ap.security === "Open" || ap.known ? connect(ap) : setPending(ap))}>
                    Connect
                  </Button>
                )}
              </div>
              {pending?.ssid === ap.ssid && (
                <form className="mt-3 flex gap-2" onSubmit={(e) => { e.preventDefault(); connect(ap, password); }}>
                  <input type="password" autoComplete="off" autoFocus placeholder={`Password for ${ap.ssid}`} value={password}
                    onChange={(e) => setPassword(e.target.value)} minLength={8}
                    className="flex-1 rounded-xl border border-border bg-bg/70 px-3 py-2 text-sm outline-none focus:border-purple" />
                  <Button small tone="primary" disabled={password.length < 8}>Join</Button>
                  <Button small onClick={() => { setPending(null); setPassword(""); }}>Cancel</Button>
                </form>
              )}
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}

function DeviceIcon({ icon }: { icon: string | null }) {
  if (icon?.startsWith("audio")) return <I.Headphones />;
  if (icon?.startsWith("input-mouse")) return <I.Mouse />;
  if (icon?.startsWith("input-keyboard")) return <I.Keyboard />;
  return <I.Bluetooth />;
}

export function BluetoothView() {
  const bt = usePoll(api.bluetooth, 5000);
  const { busy, run } = useAction();
  const s = bt.data;
  const act = (a: string, addr: string | undefined, label: string, on?: boolean) =>
    run(`${a}-${addr ?? ""}`, () => api.bt(a, addr, on), label, bt.refresh);

  return (
    <Card title="Bluetooth" icon={<I.Bluetooth />} right={s?.available && <Toggle label="Bluetooth power" on={s.powered} onChange={(on) => act("power", undefined, on ? "Bluetooth on" : "Bluetooth off", on)} />}>
      {bt.error && <p className="text-sm text-red">{bt.error}</p>}
      {s && !s.available && <Empty>No Bluetooth adapter found.</Empty>}
      {s?.available && (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-3">
            <span className="text-sm text-muted">{s.adapter} · {s.powered ? "on" : "off (on demand)"}</span>
            <div className="ml-auto">
              {s.discovering
                ? <Button small onClick={() => act("discover", undefined, "Stopped searching", false)}>Stop searching</Button>
                : <Button small tone="primary" disabled={!s.powered} onClick={() => act("discover", undefined, "Searching for 60 s; pair from the list", true)}>Search for devices</Button>}
            </div>
          </div>
          {s.devices.length === 0 && <Empty>No devices. Turn Bluetooth on and search.</Empty>}
          <ul className="grid gap-2 md:grid-cols-2">
            {s.devices.map((d) => (
              <li key={d.address} className="flex items-center gap-3 rounded-xl border border-border bg-panel-hover/30 p-3">
                <span className={d.connected ? "text-cyan" : "text-muted"}><DeviceIcon icon={d.icon} /></span>
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{d.name}</div>
                  <div className="text-xs text-muted">
                    {d.connected ? "Connected" : d.paired ? "Paired" : "Available"}
                    {d.battery_percent != null && ` · ${d.battery_percent}% battery`}
                    {d.trusted && " · trusted"}
                  </div>
                </div>
                {!d.paired && <Button small tone="primary" busy={busy === `pair-${d.address}`} onClick={() => act("pair", d.address, `Paired ${d.name}`)}>Pair</Button>}
                {d.paired && !d.connected && <Button small busy={busy === `connect-${d.address}`} onClick={() => act("connect", d.address, `Connected ${d.name}`)}>Connect</Button>}
                {d.connected && <Button small busy={busy === `disconnect-${d.address}`} onClick={() => act("disconnect", d.address, `Disconnected ${d.name}`)}>Disconnect</Button>}
                {d.paired && (
                  <Button small tone="danger" onClick={() => { if (confirm(`Forget ${d.name}? You will need to pair it again.`)) act("forget", d.address, `Forgot ${d.name}`); }}>Forget</Button>
                )}
              </li>
            ))}
          </ul>
          <p className="mt-4 text-xs text-muted">Pairing requests are only accepted for 60 seconds after you press Search or Pair.</p>
        </>
      )}
    </Card>
  );
}

export function AudioView() {
  const a = usePoll(api.audio, 8000);
  const { run } = useAction();
  const s = a.data;
  return (
    <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
      <Card title="Output device" icon={<I.Speaker />}>
        {a.error && <p className="text-sm text-red">{a.error}</p>}
        {s?.error && <p className="text-sm text-red">{s.error}</p>}
        <ul className="flex flex-col gap-2">
          {s?.sinks.map((k) => (
            <li key={k.id} className={`rounded-xl border p-3 ${k.default ? "border-purple/70 bg-purple/15" : "border-border bg-panel-hover/30"}`}>
              <div className="flex items-center gap-3">
                {k.bluetooth ? <I.Headphones className="text-cyan" /> : <I.Speaker className="text-muted" />}
                <span className="min-w-0 flex-1 truncate font-medium">{k.name}</span>
                {k.default ? <Pill tone="purple">Default</Pill> : <Button small onClick={() => run("sink", () => api.setSink(k.id), `Output: ${k.name}`, a.refresh)}>Use</Button>}
              </div>
              <input type="range" min={0} max={100} step={1} aria-label={`Volume for ${k.name}`} defaultValue={Math.round((k.volume ?? 0) * 100)}
                onPointerUp={(e) => run("vol", () => api.setVolume(k.id, Number(e.currentTarget.value) / 100))} onKeyUp={(e) => run("vol", () => api.setVolume(k.id, Number(e.currentTarget.value) / 100))} className="mt-2 w-full" />
            </li>
          ))}
        </ul>
      </Card>
      <Card title="Now playing" icon={<I.Music />}>
        {s && s.playing.length === 0 && <Empty>Nothing playing. Open Music to play in Chrome.</Empty>}
        {s?.playing.map((p, i) => (
          <div key={i} className="rounded-xl border border-border bg-panel-hover/30 p-3">
            <div className="text-xs uppercase tracking-wide text-muted">{p.player} · {p.status}</div>
            <div className="mt-1 font-semibold">{p.title ?? "Unknown title"}</div>
            {p.artist && <div className="text-sm text-muted">{p.artist}</div>}
          </div>
        ))}
        <div className="mt-3 flex gap-2">
          <Button small onClick={() => run("pp", api.playPause, undefined, a.refresh)}><span className="inline-flex items-center gap-2"><I.Play size={14} /> Play / Pause</span></Button>
          <Button small onClick={() => run("music", () => api.launch("music"), "Music opened")}>Open Music</Button>
        </div>
      </Card>
    </div>
  );
}
