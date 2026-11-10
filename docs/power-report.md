# Power report

**Status: not yet measured.** No figure below is a claim until a row comes
from `scripts/benchmark.sh` on this laptop, unplugged, after the settle time.

Machine: MSI Cyborg 15 B13WFKG · i7-13620H (Raptor Lake-P) + RTX 5060 Laptop ·
BAT1 MSI BIF0_9, 3.50 Ah design / 3.39 Ah full (97% health), ~17.5 V ·
Ubuntu 26.04, kernel 7.0.0-42.

The fuel gauge on BAT1 reports `current_now` and `voltage_now`, not
`power_now`, so watts are derived and approximate.

## Targets (from the spec)

| Profile | Target | Workload recipe |
|---|---|---|
| Deep Eco | 5–8 W (aspirational) | Eco boot, ADG session, eco profile, brightness 20%, one foot terminal, Wi-Fi associated and idle, Bluetooth off, dashboard closed |
| Developer | 8–12 W | As above, plus `claude` idle in foot and the dashboard open |
| Connected | 10–15 W | Developer, plus Bluetooth headphones playing music from Chrome (YouTube Music tab) |
| Performance | 15 W+ | Balanced or performance profile, `cargo build --release` loop |

## Procedure

```bash
# Unplug. Set up the workload. Then, for example:
scripts/benchmark.sh power-saver deep-eco-idle
scripts/benchmark.sh power-saver developer-claude-idle
scripts/benchmark.sh power-saver connected-bt-music
scripts/benchmark.sh performance build-loop
```

Each run: sets the profile, waits 60 s, samples every 5 s for 300 s,
aborts if the charger is connected, and records brightness, Wi-Fi state,
dGPU runtime status, CPU utilisation and battery health in
`reports/power-benchmarks.csv`.

## Results

| Date | Boot | Profile | Workload | Avg | Range | CPU | Brightness | Wi-Fi | dGPU | Health |
|---|---|---|---|---|---|---|---|---|---|---|
| — | — | — | *awaiting measurements* | — | — | — | — | — | — | — |

## Observations from development (not measurements)

- During development the RTX 5060 was reported `suspended` by the kernel's
  runtime PM with `control=auto`, while the NVIDIA driver's own
  `/proc/driver/nvidia/gpus/*/power` showed "Video Memory: Active". The
  dashboard shows both and trusts the kernel's `runtime_status`.
- Under GNOME, Chrome and Discord render through the dGPU on demand; the ADG
  session pins Sway to the iGPU (`WLR_DRM_DEVICES`) so the compositor never
  wakes it.
