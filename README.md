# ADG Power OS

A selectable, reversible low-power Ubuntu session for the MSI Cyborg 15 B13WFKG.
It is not a separate operating system: it adds one GRUB entry, one Wayland
session and one user daemon. Normal Ubuntu boots and behaves exactly as before.

Spec: [spec/ADG_Power_OS_Specification.pdf](spec/ADG_Power_OS_Specification.pdf) ·
handoff notes: [implementation/README.md](implementation/README.md)

![Dashboard](docs/screenshots/dashboard.png)

## What is here

| Path | What it is |
|---|---|
| `crates/adg-powerd/` | Rust daemon (unprivileged): battery telemetry, history, CPU/RAM/GPU, NetworkManager, BlueZ, PipeWire, power profiles, service matrix, eco profile with rollback, audit log. JSON API on a per-user Unix socket. |
| `crates/adg-powerd/src/bin/adg-power-helper.rs` | The only root code: two allowlisted actions (GPU runtime PM, Wi-Fi power save), run through pkexec/polkit. |
| `app/` | React + Tailwind dashboard; `app/src-tauri/` is the Tauri shell. |
| `session/` | Sway session launcher, Sway config, desktop entries. |
| `system/` | systemd user unit, eco boot generator, GRUB entry script, polkit policy. |
| `scripts/` | `discover.sh` (phase 0 audit), `install.sh` / `uninstall.sh` (dry run by default), `benchmark.sh`, `test-generator.sh`. |
| `docs/` | Deployment, rollback, service matrix, power report. |

## Architecture

```
GRUB ── "Ubuntu" (unchanged) ─────────────────────────────► normal boot
     └─ "ADG Power OS" (+ adgpower.eco=1)
            │
            ├─ adg-power-eco-generator  masks Docker/containerd/k3s/DBs/updaters/indexers
            │                           in /run only (gone on next normal boot)
            └─ login screen → "ADG Power OS" session → sway (iGPU only)
                    ├─ adg-powerd (user service) ── D-Bus ─► NetworkManager, BlueZ, logind,
                    │      │  Unix socket (0600)            power-profiles-daemon, UPower, systemd
                    │      │                        sysfs ─► power_supply, backlight, PCI runtime PM
                    │      └─ pkexec ─► adg-power-helper (root, 2 allowlisted actions)
                    └─ adg-power-dashboard (Tauri) ── invoke("daemon") ─► socket
```

The dashboard can be closed at any time; the daemon, Wi-Fi and Bluetooth keep running.

## Quick start (development, no install)

```bash
cargo test                              # daemon tests (battery math, policy, parsers)
cargo run -p adg-powerd -- once         # one reading from this laptop
cargo run -p adg-powerd -- serve        # start the daemon on $XDG_RUNTIME_DIR/adg-power/powerd.sock
cd app && pnpm install && pnpm dev      # dashboard at http://127.0.0.1:1420 (proxied to the socket)
sh scripts/test-generator.sh            # eco generator behaviour
```

With `libwebkit2gtk-4.1-dev` installed, `cd app && pnpm tauri dev` runs the native window.

## Install

See [docs/deployment.md](docs/deployment.md). Everything is dry-run first:

```bash
scripts/install.sh            # prints the plan
scripts/install.sh --apply    # session + daemon + helper + generator
scripts/install.sh --apply --grub   # also the boot entry
```

Undo with `scripts/uninstall.sh --apply` ([docs/rollback.md](docs/rollback.md)).

## Honest limits

- Battery numbers come from the fuel gauge and are estimates. This BAT1 reports
  current and charge, not power, so watts are derived (current × voltage).
- The 5–15 W budgets are targets. Nothing here claims they are met until
  `scripts/benchmark.sh` has measured them; see [docs/power-report.md](docs/power-report.md).
- The keyboard backlight on this MSI model is driven by the EC and is not
  exposed to Linux, so the eco profile cannot switch it off.
