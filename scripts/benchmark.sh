#!/usr/bin/env bash
# Phase 6: record a five-minute stabilised battery-draw average for one
# profile + workload, with the conditions that affect it. Appends a row to
# reports/power-benchmarks.csv and prints a Markdown line for docs/power-report.md.
#
#   scripts/benchmark.sh <profile> <workload> [seconds=300] [settle=60]
#   profile:  power-saver | balanced | performance
#   workload: free text, e.g. "idle-terminal", "claude-code-idle", "chrome-youtube-bt"
#
# Unplug the charger first. Set up the workload yourself before running so
# runs are reproducible (see docs/power-report.md for the workload recipes).
set -euo pipefail
cd "$(dirname "$0")/.."
PROFILE="${1:?profile}"; WORKLOAD="${2:?workload}"; SECS="${3:-300}"; SETTLE="${4:-60}"
PWD_BIN="${ADG_POWERD:-adg-powerd}"
command -v "$PWD_BIN" >/dev/null || PWD_BIN=./target/release/adg-powerd
command -v "$PWD_BIN" >/dev/null || [ -x "$PWD_BIN" ] || PWD_BIN=./target/debug/adg-powerd

BAT=$(for p in /sys/class/power_supply/*; do [ "$(cat "$p/type")" = Battery ] && [ "$(cat "$p/scope" 2>/dev/null)" != Device ] && echo "$p" && break; done)
[ -n "$BAT" ] || { echo "No battery found"; exit 1; }
[ "$(cat "$BAT/status")" = Discharging ] || { echo "Unplug the charger first (status: $(cat "$BAT/status"))."; exit 1; }

powerprofilesctl set "$PROFILE"
echo "Profile $PROFILE set. Settling ${SETTLE}s..."
sleep "$SETTLE"

read_w() {
    if [ -r "$BAT/power_now" ]; then awk '{printf "%.4f", ($1<0?-$1:$1)/1e6}' "$BAT/power_now"
    else awk -v i="$(cat "$BAT/current_now")" -v v="$(cat "$BAT/voltage_now")" 'BEGIN{printf "%.4f", (i<0?-i:i)*v/1e12}'; fi
}
cpu_busy() { awk '/^cpu /{print $2+$3+$4+$7+$8+$9, $5+$6}' /proc/stat; }

echo "Sampling ${SECS}s (every 5 s)..."
read b0 i0 < <(cpu_busy)
vals=()
for ((t=0; t<SECS; t+=5)); do
    [ "$(cat "$BAT/status")" = Discharging ] || { echo "Charger connected during run; aborting."; exit 1; }
    vals+=("$(read_w)"); sleep 5
done
read b1 i1 < <(cpu_busy)
stats=$(printf '%s\n' "${vals[@]}" | sort -n | awk '{a[NR]=$1; s+=$1} END{printf "%.2f %.2f %.2f %.2f", s/NR, a[1], a[NR], a[int((NR+1)/2)]}')
read avg min max med <<<"$stats"
cpu=$(awk -v b=$((b1-b0)) -v i=$((i1-i0)) 'BEGIN{printf "%.1f", 100*b/(b+i)}')
bl=$(for d in /sys/class/backlight/*; do awk -v m="$(cat "$d/max_brightness")" '{printf "%.0f", 100*$1/m}' "$d/brightness"; break; done)
wifi=$(nmcli -t -f WIFI g 2>/dev/null || echo unknown)
gpu=$(for d in /sys/bus/pci/devices/*; do case "$(cat "$d/class")" in 0x03*) [ "$(cat "$d/boot_vga" 2>/dev/null)" = 1 ] || cat "$d/power/runtime_status";; esac; done | head -1)
health=$(awk -v f="$(cat "$BAT/charge_full" 2>/dev/null || cat "$BAT/energy_full")" -v d="$(cat "$BAT/charge_full_design" 2>/dev/null || cat "$BAT/energy_full_design")" 'BEGIN{printf "%.0f", 100*f/d}')
eco=$(grep -qw adgpower.eco=1 /proc/cmdline && echo eco-boot || echo normal-boot)

mkdir -p reports
CSV=reports/power-benchmarks.csv
[ -f "$CSV" ] || echo "date,boot,profile,workload,seconds,avg_w,median_w,min_w,max_w,cpu_pct,brightness_pct,wifi,gpu_runtime,battery_health_pct" > "$CSV"
echo "$(date -Is),$eco,$PROFILE,$WORKLOAD,$SECS,$avg,$med,$min,$max,$cpu,$bl,$wifi,$gpu,$health" >> "$CSV"
echo
echo "| $(date +%F) | $eco | $PROFILE | $WORKLOAD | **$avg W** | $min–$max W | $cpu% | $bl% | $wifi | $gpu | $health% |"
echo "Saved to $CSV. These are estimates from the battery fuel gauge, not a calibrated meter."
