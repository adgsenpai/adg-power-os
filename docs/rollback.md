# Rollback guide

Every change ADG Power OS makes can be undone. From least to most:

| What | Undo |
|---|---|
| Eco profile (power profile, brightness, keyboard light, Bluetooth, 60 Hz) | Dashboard → "Low Power Active" → revert, or `adg-powerd call POST /api/eco/revert`. Prior values are in `~/.local/state/adg-power/revert.json`. |
| GPU runtime PM / Wi-Fi power save | Same Settings buttons with the opposite value. Both reset on reboot anyway. |
| A service stopped from the dashboard | Press Start on its row, or `sudo systemctl start <unit>`. Stops never disable or mask units. |
| Eco boot masks | Reboot into "Ubuntu". The masks live in `/run/systemd/generator.early/` and vanish at shutdown. |
| The session | Choose "Ubuntu" on the login screen. |
| Everything | `scripts/uninstall.sh` (dry run) then `scripts/uninstall.sh --apply`. Removes exactly the files listed in `/var/lib/adg-power/manifest`, removes the GRUB script and runs `update-grub`. |

## If the eco boot does not reach the login screen

1. At the GRUB menu (hold Shift / press Esc) choose **Ubuntu**. It never
   receives `adgpower.eco=1`, so the generator does nothing.
2. Or press `e` on the ADG Power OS entry and delete `adgpower.eco=1`.
3. To disable the generator outright from a normal boot:
   `sudo rm /usr/lib/systemd/system-generators/adg-power-eco-generator /usr/lib/systemd/user-generators/adg-power-eco-generator`

## Restoring GRUB by hand

```bash
ls /var/backups/adg-power/
sudo rm /etc/grub.d/42_adg_power_os
sudo update-grub            # regenerates from Ubuntu's own scripts
# or, as a last resort:
sudo cp /var/backups/adg-power/<timestamp>/grub.cfg /boot/grub/grub.cfg
```

`/etc/default/grub` is never edited; the backup is only a precaution.

## Audit trail

Every profile change, service stop/start, Wi-Fi/Bluetooth action and helper
call is appended to `~/.local/state/adg-power/audit.log` (JSON lines).
Wi-Fi passwords are never written there or anywhere else by ADG Power OS.
