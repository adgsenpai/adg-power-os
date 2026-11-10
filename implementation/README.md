# ADG Power OS — Claude implementation handoff

Implement a safe, reversible low-power Ubuntu session on MSI Cyborg 15 B13WFKG. Start with telemetry and a standalone UI. Do NOT modify GRUB or disable existing services until discovery, backup and explicit confirmation.

## Suggested milestones
1. Hardware audit: Ubuntu release, systemd units/sockets, GPU PCI IDs, Wayland compatibility, battery sysfs, audio, NetworkManager. Capture baseline battery discharge over 5 minutes unplugged.
2. Rust telemetry daemon (unprivileged) with battery power from current_now*voltage_now/1e12; energy from charge_now*voltage_now/1e12 (Wh), or energy_now/1e6 if exposed. Smooth readings and mark estimates. Handle charge/discharge, zero draw, unknown sensors.
3. React/Tailwind/Tauri desktop with live cards, power history, network selector, Bluetooth pairing, launcher, service manager and settings. No shell interpolation of SSIDs/passwords. Use DBus APIs and policy authorization.
4. Sway/Wayland session launched from a dedicated user session; keep browser/audio on demand. Validate NVIDIA hybrid graphics.
5. Dedicated boot mode with kernel cmdline marker and systemd generator/profile only after tested reversible plan. Preserve normal boot; do not globally disable Docker. Prevent socket activation and user services as necessary.
6. Benchmark 5/8/10/12/15W under reproducible workloads. Report measured vs target, screen brightness, Wi-Fi, GPU state, CPU and battery health.

## Hard requirements
- No automatic Docker, containerd, k3s, database services, VMs or indexers in eco mode; don't change normal Ubuntu defaults.
- NetworkManager stays active; Bluetooth and audio on demand.
- GPU must be verified suspended, not claimed suspended based only on a toggle.
- Power controls must have rollback, clear status and permission checks.
- Don't force-stop services with active user data or kill ongoing work.
- Battery predictions are estimates, not guarantees. 5W may be unattainable.
- Add tests for battery sensor discovery and unit conversion; avoid hard-coded BAT0.

See spec/ADG_Power_OS_Specification.pdf and assets/.
