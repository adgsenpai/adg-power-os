#!/usr/bin/env bash
# Remove everything scripts/install.sh added, restore display/power settings,
# and regenerate GRUB without the ADG Power OS entry.
#   scripts/uninstall.sh          dry run
#   scripts/uninstall.sh --apply  do it
set -euo pipefail
APPLY=0; [ "${1:-}" = "--apply" ] && APPLY=1
MANIFEST=/var/lib/adg-power/manifest
run() { echo "+ $*"; [ $APPLY -eq 1 ] && "$@" || true; }

# Restore settings changed by the eco profile, if the daemon is running.
command -v adg-powerd >/dev/null && run sh -c 'adg-powerd call POST /api/eco/revert || true'
run systemctl --user disable --now adg-powerd.service

if [ -f "$MANIFEST" ]; then
    sort -u "$MANIFEST" | while read -r f; do
        case "$f" in /usr/*|/etc/adg-power/*|/etc/grub.d/42_adg_power_os) run sudo rm -f "$f" ;; *) echo "skip unexpected path $f" ;; esac
    done
    run sudo rm -f "$MANIFEST"
else
    echo "No manifest at $MANIFEST; nothing recorded as installed."
fi
run sudo rmdir --ignore-fail-on-non-empty /usr/libexec/adg-power /usr/share/adg-power/sway /usr/share/adg-power /usr/share/backgrounds/adg-power /usr/share/doc/adg-power /etc/adg-power
[ -e /etc/grub.d/42_adg_power_os ] || run sudo update-grub
run systemctl --user daemon-reload
echo "Backups of grub.cfg (if any) remain in /var/backups/adg-power/."
