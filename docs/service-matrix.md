# Service matrix

Generated from `crates/adg-powerd/src/policy.rs` (the single source of truth; the eco generator's lists are produced from it by `adg-powerd policy --generator-list`).

Normal Ubuntu boot: **every unit keeps its original enablement.** The "Eco boot" column applies only when booted with `adgpower.eco=1`.

| Unit | Scope | Category | Eco boot | Dashboard stop guard |
|---|---|---|---|---|
| `NetworkManager.service` | system | network | Always on | never stopped from the dashboard |
| `wpa_supplicant.service` | system | network | Always on | never stopped from the dashboard |
| `systemd-logind.service` | system | core | Always on | never stopped from the dashboard |
| `power-profiles-daemon.service` | system | power | Always on | never stopped from the dashboard |
| `upower.service` | system | power | Always on | never stopped from the dashboard |
| `bluetooth.service` | system | bluetooth | On demand | never stopped from the dashboard |
| `pipewire.socket` | user | audio | On demand | never stopped from the dashboard |
| `wireplumber.service` | user | audio | On demand | never stopped from the dashboard |
| `docker.service` | system | containers | Masked at eco boot | refused while containers run |
| `docker.socket` | system | containers | Masked at eco boot | refused while containers run |
| `containerd.service` | system | containers | Masked at eco boot | refused while containers run |
| `k3s.service` | system | kubernetes | Masked at eco boot | refused while containers run |
| `k3s-agent.service` | system | kubernetes | Masked at eco boot | refused while containers run |
| `snap.microk8s.daemon-kubelite.service` | system | kubernetes | Masked at eco boot | refused while containers run |
| `postgresql.service` | system | database | Masked at eco boot | refused while clients are connected (port 5432) |
| `postgresql@.service` | system | database | Masked at eco boot | refused while clients are connected (port 5432) |
| `mysql.service` | system | database | Masked at eco boot | refused while clients are connected (port 3306) |
| `mariadb.service` | system | database | Masked at eco boot | refused while clients are connected (port 3306) |
| `redis-server.service` | system | database | Masked at eco boot | refused while clients are connected (port 6379) |
| `mongod.service` | system | database | Masked at eco boot | refused while clients are connected (port 27017) |
| `libvirtd.service` | system | virtualization | Masked at eco boot | never stopped from the dashboard |
| `libvirtd.socket` | system | virtualization | Masked at eco boot | never stopped from the dashboard |
| `virtqemud.socket` | system | virtualization | Masked at eco boot | never stopped from the dashboard |
| `snap.lxd.daemon.service` | system | virtualization | Masked at eco boot | never stopped from the dashboard |
| `ollama.service` | system | gpu | Masked at eco boot | — |
| `nvidia-persistenced.service` | system | gpu | Masked at eco boot | — |
| `nvidia-powerd.service` | system | gpu | Masked at eco boot | — |
| `unattended-upgrades.service` | system | updates | Masked at eco boot | — |
| `apt-daily.timer` | system | updates | Masked at eco boot | — |
| `apt-daily-upgrade.timer` | system | updates | Masked at eco boot | — |
| `packagekit.service` | system | updates | Masked at eco boot | — |
| `fwupd-refresh.timer` | system | updates | Masked at eco boot | — |
| `snapd.snap-repair.timer` | system | updates | Masked at eco boot | — |
| `update-notifier-download.timer` | system | updates | Masked at eco boot | — |
| `motd-news.timer` | system | updates | Masked at eco boot | — |
| `man-db.timer` | system | indexer | Masked at eco boot | — |
| `plocate-updatedb.timer` | system | indexer | Masked at eco boot | — |
| `snap.cups.cupsd.service` | system | printing | Masked at eco boot | — |
| `snap.cups.cups-browsed.service` | system | printing | Masked at eco boot | — |
| `cups-browsed.service` | system | printing | Masked at eco boot | — |
| `localsearch-3.service` | user | indexer | Masked at eco boot | — |
| `tracker-miner-fs-3.service` | user | indexer | Masked at eco boot | — |
| `evolution-data-server.service` | user | indexer | Masked at eco boot | — |

`postgresql@.service` is the template: masking it covers every cluster (`postgresql@18-main` here). `nvidia-persistenced` and `nvidia-powerd` hold the NVIDIA driver open; without them the RTX can reach runtime suspend.

Units not installed on a machine are skipped by the generator and hidden in the dashboard.
