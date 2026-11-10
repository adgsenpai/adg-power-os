import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

/** Poll an async source. Pauses while the window is hidden to save power. */
export function usePoll<T>(fn: () => Promise<T>, ms: number, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const refresh = useCallback(async () => {
    try {
      setData(await fnRef.current());
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);
  useEffect(() => {
    let t: number | undefined;
    let alive = true;
    // One request at a time per poller; the next is scheduled only after the
    // previous settles, and nothing runs while the window is hidden.
    const tick = async () => {
      if (!document.hidden) await refresh();
      if (alive) t = window.setTimeout(tick, ms);
    };
    tick();
    const onVis = () => !document.hidden && refresh();
    document.addEventListener("visibilitychange", onVis);
    return () => {
      alive = false;
      window.clearTimeout(t);
      document.removeEventListener("visibilitychange", onVis);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ms, refresh, ...deps]);
  return { data, error, refresh };
}

type Toast = { id: number; text: string; tone: "ok" | "err" };
const ToastCtx = createContext<(text: string, tone?: "ok" | "err") => void>(() => {});
export const useToast = () => useContext(ToastCtx);

export function ToastHost({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = useCallback((text: string, tone: "ok" | "err" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, tone }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="fixed bottom-5 right-5 z-50 flex flex-col gap-2" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`card max-w-sm px-4 py-3 text-sm shadow-xl ${t.tone === "err" ? "border-red/60 text-red" : "border-green/50"}`}>
            {t.text}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

/** Run an action with a toast for the result. */
export function useAction() {
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const run = useCallback(
    async (key: string, fn: () => Promise<unknown>, ok?: string, after?: () => void) => {
      setBusy(key);
      try {
        await fn();
        if (ok) toast(ok);
      } catch (e) {
        toast((e as Error).message, "err");
      } finally {
        setBusy(null);
        after?.();
      }
    },
    [toast],
  );
  return { busy, run };
}

export function Card({ title, icon, right, children, className = "" }: { title?: ReactNode; icon?: ReactNode; right?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`card p-5 ${className}`}>
      {(title || right) && (
        <header className="mb-3 flex items-center gap-3">
          {icon && <span className="text-cyan">{icon}</span>}
          <h2 className="text-[17px] font-semibold">{title}</h2>
          <div className="ml-auto">{right}</div>
        </header>
      )}
      {children}
    </section>
  );
}

export function Toggle({ on, onChange, label, disabled }: { on: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={`relative h-7 w-12 shrink-0 rounded-full transition-colors disabled:opacity-40 ${on ? "bg-green" : "bg-border"}`}
    >
      <span className={`absolute top-1 h-5 w-5 rounded-full bg-white shadow transition-all ${on ? "left-6" : "left-1"}`} />
    </button>
  );
}

export function Button({ children, onClick, tone = "default", disabled, busy, small }: {
  children: ReactNode; onClick?: () => void; tone?: "default" | "primary" | "danger"; disabled?: boolean; busy?: boolean; small?: boolean;
}) {
  const tones = {
    default: "border-border bg-panel-hover/60 hover:bg-panel-hover",
    primary: "border-purple/70 bg-purple/25 hover:bg-purple/40",
    danger: "border-red/50 bg-red/10 text-red hover:bg-red/20",
  };
  return (
    <button onClick={onClick} disabled={disabled || busy}
      className={`rounded-xl border font-medium transition disabled:cursor-not-allowed disabled:opacity-40 ${small ? "px-3 py-1.5 text-sm" : "px-4 py-2"} ${tones[tone]}`}>
      {busy ? "Working…" : children}
    </button>
  );
}

export function Bar({ value, tone = "cyan", label }: { value: number; tone?: "cyan" | "green" | "purple" | "amber"; label: string }) {
  const color = { cyan: "bg-cyan", green: "bg-green", purple: "bg-purple", amber: "bg-amber" }[tone];
  return (
    <div className="h-2 w-full overflow-hidden rounded-full bg-border/70" role="progressbar" aria-label={label}
      aria-valuenow={Math.round(value)} aria-valuemin={0} aria-valuemax={100}>
      <div className={`h-full rounded-full ${color} transition-[width] duration-700`} style={{ width: `${Math.max(1.5, Math.min(100, value))}%` }} />
    </div>
  );
}

export function Pill({ children, tone = "muted" }: { children: ReactNode; tone?: "green" | "cyan" | "amber" | "red" | "muted" | "purple" }) {
  const t = {
    green: "text-green border-green/40 bg-green/10",
    cyan: "text-cyan border-cyan/40 bg-cyan/10",
    amber: "text-amber border-amber/40 bg-amber/10",
    red: "text-red border-red/40 bg-red/10",
    purple: "text-purple border-purple/40 bg-purple/10",
    muted: "text-muted border-border bg-panel-hover/40",
  }[tone];
  return <span className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-medium ${t}`}>{children}</span>;
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="py-6 text-center text-sm text-muted">{children}</p>;
}
