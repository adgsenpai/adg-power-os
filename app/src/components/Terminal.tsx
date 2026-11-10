import { useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { api, inTauri } from "../api";
import * as I from "../icons";
import { Button, Card, useAction } from "./ui";

type Tab = { key: number; title: string; command?: "claude" };

const theme = {
  background: "#0b101b", foreground: "#edf1ff", cursor: "#a855f7", cursorAccent: "#0b101b",
  selectionBackground: "#a855f755", black: "#161c2d", brightBlack: "#5b6380",
  red: "#ff6b81", green: "#5ee8a0", yellow: "#f5c451", blue: "#55d9ff", magenta: "#a855f7",
  cyan: "#55d9ff", white: "#edf1ff", brightWhite: "#ffffff",
};

/** One xterm.js view bound to a PTY in the Tauri process. */
function Pane({ tab, active, onExit }: { tab: Tab; active: boolean; onExit: () => void }) {
  const host = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const termRef = useRef<XTerm | null>(null);

  useEffect(() => {
    let id: number | null = null;
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const term = new XTerm({
      fontFamily: '"JetBrains Mono", "Fira Code", "DejaVu Sans Mono", monospace',
      fontSize: 13, cursorBlink: false, // a blinking cursor repaints twice a second
      theme, scrollback: 3000, allowProposedApi: false,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host.current!);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      unlisten.push(await listen<{ id: number; data: string }>("pty-output", (e) => { if (e.payload.id === id) term.write(e.payload.data); }));
      unlisten.push(await listen<number>("pty-exit", (e) => { if (e.payload === id) onExit(); }));
      if (disposed) return;
      id = await invoke<number>("pty_open", { cols: term.cols, rows: term.rows, command: tab.command ?? null });
      term.onData((d) => id != null && invoke("pty_write", { id, data: d }));
      term.onResize(({ cols, rows }) => id != null && invoke("pty_resize", { id, cols, rows }));
      term.focus();
    })().catch((e) => term.write(`\r\n\x1b[31m${(e as Error).message ?? e}\x1b[0m\r\n`));

    const ro = new ResizeObserver(() => { try { fit.fit(); } catch { /* hidden */ } });
    ro.observe(host.current!);
    return () => {
      disposed = true;
      ro.disconnect();
      unlisten.forEach((u) => u());
      if (id != null) import("@tauri-apps/api/core").then(({ invoke }) => invoke("pty_close", { id }));
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (active) {
      requestAnimationFrame(() => { try { fitRef.current?.fit(); } catch { /* */ } termRef.current?.focus(); });
    }
  }, [active]);

  return <div ref={host} className={`h-full w-full ${active ? "" : "hidden"}`} />;
}

let nextKey = 1;

export function TerminalView({ request }: { request: { what: "claude" | "shell"; n: number } | null }) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const { run } = useAction();

  const open = (command?: "claude") => {
    const t: Tab = { key: nextKey++, title: command === "claude" ? "Claude Code" : "Shell", command };
    setTabs((ts) => [...ts, t]);
    setActive(t.key);
  };
  const close = (key: number) => {
    setTabs((ts) => {
      const rest = ts.filter((t) => t.key !== key);
      setActive((a) => (a === key ? rest[rest.length - 1]?.key ?? null : a));
      return rest;
    });
  };

  // Each request from a launcher tile opens a new tab; first visit opens a shell.
  useEffect(() => {
    if (!inTauri) return;
    if (request) open(request.what === "claude" ? "claude" : undefined);
    else if (tabs.length === 0) open();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request?.n]);

  if (!inTauri) {
    return (
      <Card title="Terminal" icon={<I.Terminal />}>
        <p className="text-sm text-muted">The embedded terminal needs the native dashboard window. In the browser preview, open an external one instead.</p>
        <div className="mt-3 flex gap-2">
          <Button tone="primary" onClick={() => run("t", () => api.launch("claude"), "Claude Code opened")}>Claude Code in foot</Button>
          <Button onClick={() => run("t", () => api.launch("terminal"), "Terminal opened")}>Shell in foot</Button>
        </div>
      </Card>
    );
  }

  return (
    <section className="card flex h-[calc(100vh-8.5rem)] min-h-[360px] flex-col overflow-hidden">
      <header className="flex items-center gap-1 border-b border-border px-2 py-1.5">
        {tabs.map((t) => (
          <div key={t.key} className={`flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm ${active === t.key ? "bg-purple/25 text-text" : "text-muted hover:bg-panel-hover"}`}>
            <button onClick={() => setActive(t.key)} className="flex items-center gap-2">
              {t.command === "claude" ? <I.Sparkle size={14} className="text-purple" /> : <I.Terminal size={14} />}{t.title}
            </button>
            <button aria-label={`Close ${t.title}`} onClick={() => close(t.key)} className="ml-1 rounded px-1 text-muted hover:text-red">×</button>
          </div>
        ))}
        <button onClick={() => open()} className="rounded-lg px-3 py-1.5 text-sm text-muted hover:bg-panel-hover" title="New shell">+ Shell</button>
        <button onClick={() => open("claude")} className="rounded-lg px-3 py-1.5 text-sm text-purple hover:bg-panel-hover" title="New Claude Code tab">+ Claude</button>
        <button onClick={() => run("t", () => api.launch("terminal"), "Terminal opened")} className="ml-auto rounded-lg px-3 py-1.5 text-xs text-muted hover:bg-panel-hover">Open in separate window</button>
      </header>
      <div className="min-h-0 flex-1 bg-bg p-2">
        {tabs.length === 0 && <div className="grid h-full place-items-center text-sm text-muted">No terminals open.</div>}
        {tabs.map((t) => <Pane key={t.key} tab={t} active={active === t.key} onExit={() => close(t.key)} />)}
      </div>
    </section>
  );
}
