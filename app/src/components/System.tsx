import { Fragment } from "react";
import { api, type ServiceRow, type Status } from "../api";
import { percent, profileLabel } from "../format";
import * as I from "../icons";
import { Button, Card, Pill, useAction, usePoll } from "./ui";

const ecoLabel: Record<ServiceRow["eco"], { text: string; tone: "red" | "cyan" | "green" }> = {
  blocked_at_eco_boot: { text: "Off in eco boot", tone: "red" },
  on_demand: { text: "On demand", tone: "cyan" },
  keep: { text: "Always on", tone: "green" },
};

export function ServicesView() {
  const svc = usePoll(api.services, 10000);
  const { busy, run } = useAction();
  const rows = svc.data?.services.filter((r) => r.installed) ?? [];
  const missing = svc.data?.services.filter((r) => !r.installed).length ?? 0;

  const stop = (r: ServiceRow) => {
    if (!confirm(`Stop ${r.unit} now? This only affects the running system; it does not disable the service, and normal boot is unchanged.`)) return;
    run(`stop-${r.unit}`, () => api.serviceStop(r.unit), `${r.unit} stopped`, svc.refresh);
  };

  return (
    <Card title="Service matrix" icon={<I.Shield />} right={svc.data && <span className="text-xs text-muted">{svc.data.running_containers} running container(s)</span>}>
      {svc.error && <p className="text-sm text-red">{svc.error}</p>}
      <p className="mb-4 text-sm text-muted">
        "Off in eco boot" units are masked only for the ADG Power OS boot entry, through a runtime systemd generator. Normal Ubuntu keeps its original behaviour. Stopping here asks systemd's own permission prompt and is refused while work is active.
      </p>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead className="text-xs uppercase tracking-wide text-muted">
            <tr><th className="py-2 pr-3">Unit</th><th className="pr-3">Category</th><th className="pr-3">Eco policy</th><th className="pr-3">Now</th><th className="pr-3">At boot</th><th /></tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.unit} className="border-t border-border/60">
                <td className="py-2 pr-3"><div className="font-medium">{r.unit}</div><div className="text-xs text-muted">{r.description} · {r.scope}</div></td>
                <td className="pr-3 text-muted">{r.category}</td>
                <td className="pr-3"><Pill tone={ecoLabel[r.eco].tone}>{ecoLabel[r.eco].text}</Pill></td>
                <td className="pr-3">{r.active_state === "active" ? <span className="text-green">{r.sub_state}</span> : <span className="text-muted">{r.active_state}</span>}</td>
                <td className="pr-3 text-muted">{r.unit_file_state || "—"}</td>
                <td className="py-2 text-right">
                  {r.eco !== "keep" && (r.active_state === "active"
                    ? <span title={r.stop_blocker ?? undefined}><Button small tone="danger" disabled={!r.stoppable} busy={busy === `stop-${r.unit}`} onClick={() => stop(r)}>Stop</Button></span>
                    : <Button small busy={busy === `start-${r.unit}`} onClick={() => run(`start-${r.unit}`, () => api.serviceStart(r.unit), `${r.unit} started`, svc.refresh)}>Start</Button>)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {rows.some((r) => r.stop_blocker && r.eco !== "keep" && r.active_state === "active") && (
        <ul className="mt-3 list-disc pl-5 text-xs text-amber">
          {rows.filter((r) => r.stop_blocker && r.eco !== "keep" && r.active_state === "active").map((r) => <li key={r.unit}>{r.unit}: {r.stop_blocker}</li>)}
        </ul>
      )}
      <p className="mt-3 text-xs text-muted">{missing} other policy units are not installed on this machine.</p>
    </Card>
  );
}

export function SettingsView({ status, refresh }: { status: Status; refresh: () => void }) {
  const audit = usePoll(api.audit, 15000);
  const { busy, run } = useAction();
  const kb = status.keyboard_backlight;
  const wifiIface = usePoll(api.wifi, 30000).data?.interface;

  return (
    <div className="grid gap-5 lg:grid-cols-2">
      <Card title="Power profile" icon={<I.Leaf />}>
        <div className="flex flex-wrap gap-2">
          {(["power-saver", "balanced", "performance"] as const).map((p) => (
            <Button key={p} tone={status.profile.active === p ? "primary" : "default"} disabled={!status.profile.choices.includes(p)}
              onClick={() => run("p", () => api.setProfile(p), `${profileLabel(p)} profile`, refresh)}>{profileLabel(p)}</Button>
          ))}
        </div>
        {!status.profile.available && <p className="mt-2 text-sm text-amber">power-profiles-daemon is not running.</p>}
        <div className="mt-5 rounded-xl border border-border bg-panel-hover/30 p-4">
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <div className="font-medium">Eco profile</div>
              <div className="text-sm text-muted">Power saver, 20% brightness, keyboard light off, Bluetooth off if idle, 60 Hz under Sway. Previous values are saved and restored on revert.</div>
            </div>
            {status.eco_applied
              ? <Button tone="danger" busy={busy === "rv"} onClick={() => run("rv", api.ecoRevert, "Reverted to previous settings", refresh)}>Revert</Button>
              : <Button tone="primary" busy={busy === "ap"} onClick={() => run("ap", api.ecoApply, "Eco profile applied", refresh)}>Apply</Button>}
          </div>
        </div>
      </Card>

      <Card title="Display & keyboard" icon={<I.Monitor />}>
        <label className="block text-sm">
          <div className="mb-1 flex justify-between"><span>Brightness ({status.backlight?.device ?? "none"})</span><span className="num">{percent(status.backlight?.percent)}</span></div>
          <input type="range" className="w-full" min={5} max={100} step={5} disabled={!status.backlight}
            defaultValue={Math.round(status.backlight?.percent ?? 50)} onChange={(e) => run("b", () => api.setBrightness(Number(e.target.value)), undefined, refresh)} />
        </label>
        <div className="mt-4 text-sm">
          <div className="mb-1">Keyboard backlight</div>
          {kb ? (
            <input type="range" className="w-full" min={0} max={kb[1]} defaultValue={kb[0]} onChange={(e) => run("k", () => api.setKeyboard(Number(e.target.value)), undefined, refresh)} />
          ) : (
            <p className="text-muted">Not exposed by this laptop's firmware (MSI RGB keyboards are driven by the EC). Turn it off with Fn keys.</p>
          )}
        </div>
      </Card>

      <Card title="Privileged settings" icon={<I.Lock />}>
        <p className="mb-3 text-sm text-muted">These go through a small root helper with a fixed allowlist. Your system asks for your password each time (polkit).</p>
        <div className="flex flex-wrap gap-2">
          {wifiIface && <>
            <Button small onClick={() => run("wps", () => api.helper("wifi-powersave", wifiIface, "on"), "Wi-Fi power saving on")}>Wi-Fi power save on</Button>
            <Button small onClick={() => run("wpo", () => api.helper("wifi-powersave", wifiIface, "off"), "Wi-Fi power saving off (low latency)")}>Off (low latency)</Button>
          </>}
          {status.gpus.map((g) => (
            <Button key={g.pci_address} small onClick={() => run("g", () => api.helper("gpu-runtime-pm", g.pci_address, "auto"), "GPU runtime PM set to auto", refresh)}>
              Allow {g.vendor} GPU suspend
            </Button>
          ))}
        </div>
        <div className="mt-4 grid grid-cols-[8rem_1fr] gap-y-1 text-sm">
          {status.gpus.map((g) => (
            <Fragment key={g.pci_address}><span className="text-muted">GPU {g.pci_address}</span>
              <span>runtime_status <b>{g.runtime_status}</b>, control <b>{g.runtime_control ?? "?"}</b>{g.nvidia_video_memory && <>, VRAM <b>{g.nvidia_video_memory}</b></>}</span></Fragment>
          ))}
          <span className="text-muted">Boot mode</span><span>{status.eco_boot ? "ADG Power OS (eco boot marker present)" : "Normal Ubuntu"}</span>
        </div>
      </Card>

      <Card title="Audit log" icon={<I.Shield />} right={<span className="text-xs text-muted">last 50 changes</span>}>
        <ul className="max-h-72 overflow-y-auto text-sm">
          {(audit.data ?? []).slice().reverse().map((e, i) => (
            <li key={i} className="flex gap-3 border-t border-border/50 py-1.5">
              <span className="num shrink-0 text-muted">{new Date(e.t * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
              <span className={e.ok ? "" : "text-red"}>{e.action}</span>
              <span className="truncate text-muted">{e.detail}</span>
            </li>
          ))}
          {audit.data?.length === 0 && <li className="py-4 text-center text-muted">No changes recorded yet.</li>}
        </ul>
      </Card>
    </div>
  );
}
