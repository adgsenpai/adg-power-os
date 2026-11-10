import { api } from "../api";
import { mib } from "../format";
import * as I from "../icons";
import { Button, Card, Empty, Pill, useAction, usePoll } from "./ui";

const label: Record<string, string> = {
  terminal: "Terminal", claude: "Claude Code", chrome: "Chrome", music: "Music", files: "Files", "network-editor": "Network editor", lock: "Screen lock",
};

export function AppsView() {
  const apps = usePoll(api.apps, 5000);
  const hogs = usePoll(api.hogs, 10000);
  const { busy, run } = useAction();
  const list = apps.data?.apps ?? [];
  const total = list.reduce((s, a) => s + (a.memory_bytes ?? 0), 0);

  return (
    <div className="grid gap-5 xl:grid-cols-2">
      <Card title="Running apps" icon={<I.Grid />} right={<span className="text-xs text-muted">{list.length} open · {mib(total)}</span>}>
        <p className="mb-3 text-sm text-muted">
          Close stops the whole app, including background and helper processes, so its memory is freed.
          {apps.data?.sway ? " Apps whose windows are all closed are stopped automatically after about 30 seconds." : ""}
        </p>
        {apps.error && <p className="text-sm text-red">{apps.error}</p>}
        {list.length === 0 && <Empty>Nothing launched from the dashboard is running.</Empty>}
        <ul className="flex flex-col gap-2">
          {list.map((a) => (
            <li key={a.unit} className="flex items-center gap-3 rounded-xl border border-border bg-panel-hover/30 p-3">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{label[a.app] ?? a.app}</div>
                <div className="num text-xs text-muted">
                  {mib(a.memory_bytes)} · {a.tasks ?? "?"} processes · {a.cpu_seconds?.toFixed(0) ?? "?"} s CPU
                  {a.windows != null && ` · ${a.windows} window${a.windows === 1 ? "" : "s"}`}
                </div>
              </div>
              {a.windows === 0 && <Pill tone="amber">no windows</Pill>}
              <Button small tone="danger" busy={busy === a.unit} onClick={() => run(a.unit, () => api.closeApp(a.unit), `${label[a.app] ?? a.app} closed`, apps.refresh)}>Close</Button>
            </li>
          ))}
        </ul>
      </Card>

      <Card title="What's using power" icon={<I.Bolt />} right={<span className="text-xs text-muted">CPU over the last second</span>}>
        {hogs.error && <p className="text-sm text-red">{hogs.error}</p>}
        {hogs.data && (hogs.data.gpu_services.length > 0 || hogs.data.gpu_users.length > 0) && (
          <div className="mb-4 rounded-xl border border-amber/40 bg-amber/10 p-3 text-sm">
            <div className="flex items-center gap-2 font-medium text-amber"><I.Gpu size={18} /> Holding the RTX 5060 driver open</div>
            <ul className="mt-1 list-disc pl-5 text-muted">
              {hogs.data.gpu_services.map((s) => <li key={s}><code>{s}</code> (system service, off in Eco boot)</li>)}
              {hogs.data.gpu_users.map((u) => <li key={u.pid}>{u.name} <span className="text-xs">(pid {u.pid})</span></li>)}
            </ul>
            <p className="mt-1 text-xs text-muted">These can block or keep waking the dGPU's runtime suspend, which costs several watts each time. All are off in Eco mode.</p>
          </div>
        )}
        <table className="w-full text-sm">
          <thead className="text-xs uppercase tracking-wide text-muted"><tr><th className="py-1 text-left">Process</th><th className="text-right">CPU</th><th className="text-right">Memory</th></tr></thead>
          <tbody>
            {hogs.data?.top.map((p) => (
              <tr key={p.pid} className="border-t border-border/50">
                <td className="py-1.5">{p.name}</td>
                <td className="num text-right">
                  <span className={p.cpu_percent > 50 ? "text-red" : p.cpu_percent > 15 ? "text-amber" : ""}>{p.cpu_percent.toFixed(0)}%</span>
                </td>
                <td className="num text-right text-muted">{mib(p.rss_bytes)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-2 text-xs text-muted">100% = one full core. Sustained CPU is the biggest cause of draw above 12 W.</p>
      </Card>
    </div>
  );
}
