#!/usr/bin/env bash
# Phase 0: read-only hardware and service audit. Changes nothing.
#   scripts/discover.sh                 write reports/hardware-audit-<date>.md
#   scripts/discover.sh --baseline 300  also sample battery draw for 300 s (unplug first)
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p reports
OUT="reports/hardware-audit-$(date +%Y%m%d-%H%M).md"
BASELINE=0
[ "${1:-}" = "--baseline" ] && BASELINE="${2:-300}"

section() { printf '\n## %s\n\n```\n' "$1"; }
end() { printf '```\n'; }

{
echo "# ADG Power OS hardware audit"
echo
echo "Generated $(date -Is) on $(hostname)."
section "Release";        lsb_release -ds 2>/dev/null; uname -r; cat /sys/class/dmi/id/product_name 2>/dev/null; end
section "Kernel cmdline"; cat /proc/cmdline; end
section "Session";        echo "XDG_SESSION_TYPE=${XDG_SESSION_TYPE:-}"; echo "XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-}"; command -v sway foot kitty || true; end
section "GPUs";           lspci -nn | grep -Ei 'vga|3d|display'; end
section "dGPU runtime PM"
for d in /sys/bus/pci/devices/*; do
    case "$(cat "$d/class" 2>/dev/null)" in 0x03*) ;; *) continue ;; esac
    [ "$(cat "$d/boot_vga" 2>/dev/null)" = 1 ] && continue
    echo "$(basename "$d") status=$(cat "$d/power/runtime_status") control=$(cat "$d/power/control") driver=$(basename "$(readlink "$d/driver")")"
done
cat /proc/driver/nvidia/gpus/*/power 2>/dev/null
end
section "Power supplies"
for p in /sys/class/power_supply/*; do echo "== $(basename "$p")"; grep -v SERIAL "$p/uevent" 2>/dev/null; done
end
section "Backlight / keyboard LEDs"; ls /sys/class/backlight; ls /sys/class/leds | grep -i kbd || echo "no kbd_backlight LED"; end
section "Power profiles"; powerprofilesctl list 2>&1; end
section "Network";        nmcli -t -f DEVICE,TYPE,STATE,CONNECTION device 2>&1; end
section "Bluetooth";      bluetoothctl show 2>&1 | grep -E 'Name|Powered|Discoverable' ; end
section "Audio";          wpctl status 2>&1 | sed -n '/^Audio/,/^Video/p' | head -30; end
section "Enabled system units (eco-relevant)"
systemctl list-unit-files --state=enabled --no-pager --no-legend | grep -Ei 'docker|containerd|k3s|kube|postgres|mysql|maria|redis|mongo|libvirt|lxd|ollama|snap|apt|unattended|packagekit|fwupd|cups|tracker|localsearch|bluetooth|NetworkManager' ; end
section "Enabled timers"; systemctl list-timers --all --no-pager --no-legend | awk '{print $NF, $(NF-1)}' | head -40; end
section "Enabled user units"; systemctl --user list-unit-files --state=enabled --no-pager --no-legend; end
section "Running containers"; pgrep -c containerd-shim 2>/dev/null || echo 0; end
} > "$OUT"

if [ "$BASELINE" -gt 0 ]; then
    BAT=$(grep -l '^Battery$' /sys/class/power_supply/*/type | head -1 | xargs dirname)
    if [ "$(cat "$BAT/status")" != "Discharging" ]; then
        echo "Baseline skipped: unplug the charger first." | tee -a "$OUT"
    else
        echo "Sampling $BAT for ${BASELINE}s..."
        {
            echo; echo "## Baseline discharge (${BASELINE}s, 5 s samples)"; echo
            echo '| t (s) | W |'; echo '|---|---|'
        } >> "$OUT"
        sum=0; n=0
        for ((t=0; t<BASELINE; t+=5)); do
            if [ -r "$BAT/power_now" ]; then w=$(awk '{printf "%.3f", $1/1e6}' "$BAT/power_now")
            else w=$(awk -v i="$(cat "$BAT/current_now")" -v v="$(cat "$BAT/voltage_now")" 'BEGIN{printf "%.3f", (i<0?-i:i)*v/1e12}'); fi
            echo "| $t | $w |" >> "$OUT"
            sum=$(awk -v a="$sum" -v b="$w" 'BEGIN{print a+b}'); n=$((n+1)); sleep 5
        done
        echo; echo "**Average: $(awk -v s="$sum" -v n="$n" 'BEGIN{printf "%.2f", s/n}') W** over $n samples (estimate)." >> "$OUT"
    fi
fi
echo "Wrote $OUT"
