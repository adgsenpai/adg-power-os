#!/usr/bin/env bash
# Install ADG Power OS. Default is a dry run that prints every step.
#
#   scripts/install.sh                 show the plan, change nothing
#   scripts/install.sh --apply         install the session, daemon, helper and generator
#   scripts/install.sh --apply --grub  also add the "ADG Power OS" GRUB entry
#
# Run as your normal user; sudo is used only for the system file copies.
# Every installed path is recorded in /var/lib/adg-power/manifest so
# scripts/uninstall.sh can remove exactly what was added.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
APPLY=0; GRUB=0
for a in "$@"; do
    case "$a" in
        --apply) APPLY=1 ;;
        --grub) GRUB=1 ;;
        *) echo "unknown option $a"; exit 2 ;;
    esac
done
[ "$(id -u)" -ne 0 ] || { echo "Run as your normal user, not root."; exit 1; }

MANIFEST=/var/lib/adg-power/manifest
BACKUP=/var/backups/adg-power/$(date +%Y%m%d-%H%M%S)

run() { echo "+ $*"; [ $APPLY -eq 1 ] && "$@" || true; }
put() { # put SRC DEST MODE
    echo "+ install -m $3 $1 $2"
    if [ $APPLY -eq 1 ]; then
        sudo install -D -m "$3" -o root -g root "$1" "$2"
        echo "$2" | sudo tee -a "$MANIFEST" >/dev/null
    fi
}

echo "== ADG Power OS install ($([ $APPLY -eq 1 ] && echo APPLY || echo DRY RUN))"

# ---- 0. prerequisites
missing=()
for b in sway foot swaybg swayidle swaylock iw pkexec; do command -v "$b" >/dev/null || missing+=("$b"); done
# Ubuntu 26.04 ships the agent in /usr/libexec; older releases in the multiarch dir.
ls /usr/libexec/polkit-mate-authentication-agent-1 /usr/lib/*/polkit-mate/polkit-mate-authentication-agent-1 >/dev/null 2>&1 || missing+=("mate-polkit")
if [ ${#missing[@]} -gt 0 ]; then
    echo "Missing: ${missing[*]}"
    echo "  sudo apt install sway foot swaybg swayidle swaylock iw mate-polkit"
    [ $APPLY -eq 1 ] && exit 1
fi

# ---- 1. build
run cargo build --release -p adg-powerd
DASH_BIN=""
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    run bash -c "cd app && pnpm install --frozen-lockfile && pnpm tauri build --no-bundle"
    DASH_BIN="$ROOT/target/release/adg-power-dashboard"
else
    echo "! libwebkit2gtk-4.1-dev not installed: skipping the Tauri dashboard binary."
    echo "  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev"
fi

# ---- 2. system files
[ $APPLY -eq 1 ] && { sudo mkdir -p /var/lib/adg-power "$BACKUP"; sudo touch "$MANIFEST"; }
put target/release/adg-powerd               /usr/bin/adg-powerd 0755
put target/release/adg-power-helper         /usr/libexec/adg-power/adg-power-helper 0755
put session/igpu-drm-path                   /usr/libexec/adg-power/igpu-drm-path 0755
put session/adg-power-session               /usr/bin/adg-power-session 0755
[ -n "$DASH_BIN" ] && put "$DASH_BIN"       /usr/bin/adg-power-dashboard 0755
put system/polkit/com.adgstudios.power.policy /usr/share/polkit-1/actions/com.adgstudios.power.policy 0644
put system/systemd/adg-powerd.service       /usr/lib/systemd/user/adg-powerd.service 0644
put system/generators/adg-power-eco-generator /usr/lib/systemd/system-generators/adg-power-eco-generator 0755
put system/generators/adg-power-eco-generator /usr/lib/systemd/user-generators/adg-power-eco-generator 0755
put session/adg-power.desktop               /usr/share/wayland-sessions/adg-power.desktop 0644
put session/adg-power-dashboard.desktop     /usr/share/applications/adg-power-dashboard.desktop 0644
put session/sway/config                     /usr/share/adg-power/sway/config 0644
put assets/graphics/adg-power-logo.svg      /usr/share/icons/hicolor/scalable/apps/adg-power.svg 0644
for w in assets/wallpapers/*.png; do put "$w" "/usr/share/backgrounds/adg-power/$(basename "$w")" 0644; done
for d in docs/*.md; do put "$d" "/usr/share/doc/adg-power/$(basename "$d")" 0644; done

# Eco mask lists come from the daemon's policy table (single source of truth).
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
if [ -x target/release/adg-powerd ] || [ $APPLY -eq 0 ]; then
    BIN=target/release/adg-powerd; [ -x "$BIN" ] || BIN=target/debug/adg-powerd
    if [ -x "$BIN" ]; then
        { echo "# Units masked only when booted with adgpower.eco=1. Edit freely."; "$BIN" policy --generator-list system; } > "$TMP/eco-mask-system.list"
        { echo "# User units masked only when booted with adgpower.eco=1."; "$BIN" policy --generator-list user; } > "$TMP/eco-mask-user.list"
        for l in system user; do
            if [ -e "/etc/adg-power/eco-mask-$l.list" ]; then echo "= keeping existing /etc/adg-power/eco-mask-$l.list"
            else put "$TMP/eco-mask-$l.list" "/etc/adg-power/eco-mask-$l.list" 0644; fi
        done
    fi
fi

# ---- 3. GRUB entry (optional, explicit)
if [ $GRUB -eq 1 ]; then
    echo "== GRUB: adds one menu entry; the normal and recovery entries are untouched"
    run sudo cp -a /boot/grub/grub.cfg "$BACKUP/grub.cfg"
    run sudo cp -a /etc/default/grub "$BACKUP/default-grub"
    put system/grub/42_adg_power_os /etc/grub.d/42_adg_power_os 0755
    run sudo update-grub
    if [ $APPLY -eq 1 ]; then
        grep -q "menuentry 'ADG Power OS" /boot/grub/grub.cfg && echo "✓ ADG Power OS entry present" || { echo "✗ entry missing, restoring backup"; sudo cp "$BACKUP/grub.cfg" /boot/grub/grub.cfg; exit 1; }
        grep -q "^menuentry 'Ubuntu'" /boot/grub/grub.cfg && echo "✓ Ubuntu entry unchanged" || true
    fi
    echo "  GRUB menu: if it is hidden, hold Shift (BIOS) or press Esc (UEFI) at boot."
else
    echo "= GRUB untouched (add --grub to create the ADG Power OS boot entry)"
fi

# ---- 4. user service (no sudo)
run systemctl --user daemon-reload
run systemctl --user enable adg-powerd.service
echo
echo "Done. Log out and choose 'ADG Power OS' on the login screen gear menu."
echo "Normal Ubuntu, Docker and every existing service keep their current behaviour."
