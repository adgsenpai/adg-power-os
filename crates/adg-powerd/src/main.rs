//! adg-powerd: unprivileged telemetry and control daemon for ADG Power OS.
//!
//!   adg-powerd               run the daemon (Unix socket API + sampler)
//!   adg-powerd once [--json] print one battery/system reading and exit
//!   adg-powerd watch         print a reading every few seconds
//!   adg-powerd call METHOD PATH [JSON]
//!                            send one request to the running daemon
//!   adg-powerd policy --generator-list [system|user]
//!                            print units the eco boot generator masks

use adg_power::{battery, daemon, policy, server, system};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn usage() -> ! {
    eprintln!("usage: adg-powerd [serve [--socket PATH] | once [--json] | watch | call METHOD PATH [JSON] | policy --generator-list [system|user]]");
    std::process::exit(2)
}

fn fmt_opt(v: Option<f64>, unit: &str) -> String {
    v.map(|v| format!("{v:.2} {unit}")).unwrap_or_else(|| "n/a".into())
}

fn print_once(root: &Path, json: bool) {
    let snap = battery::snapshot(root);
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "power": snap,
            "memory": system::read_memory(),
            "gpus": system::read_gpus(),
            "backlight": system::read_backlight(Path::new("/sys/class/backlight")),
        })).unwrap());
        return;
    }
    if snap.batteries.is_empty() {
        println!("No system battery found under {}", root.display());
    }
    for b in &snap.batteries {
        println!(
            "{}: {} {}  power {} ({:?})  energy {} / {} ({:?})  health {}",
            b.name,
            b.status_raw,
            b.capacity_percent.map(|c| format!("{c:.0}%")).unwrap_or_default(),
            fmt_opt(b.power_w, "W"),
            b.power_source,
            fmt_opt(b.energy_now_wh, "Wh"),
            fmt_opt(b.energy_full_wh, "Wh"),
            b.energy_source,
            b.health_percent().map(|h| format!("{h:.0}%")).unwrap_or_else(|| "n/a".into()),
        );
    }
    println!("state {:?}  ac {:?}  estimated {}", snap.state, snap.ac_online, snap.estimated);
    for g in system::read_gpus() {
        println!("GPU {} {} runtime_status={} control={:?} driver={:?}", g.pci_address, g.vendor, g.runtime_status, g.runtime_control, g.driver);
    }
}

/// Minimal client for scripts and the Sway session (no curl dependency).
fn call(socket: &Path, method: &str, path: &str, body: Option<&str>) -> std::io::Result<String> {
    use std::io::{Read, Write};
    let mut s = std::os::unix::net::UnixStream::connect(socket)?;
    let body = body.unwrap_or("");
    write!(s, "{method} {path} HTTP/1.1\r\nHost: adg-powerd\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    let (_, json) = out.split_once("\r\n\r\n").unwrap_or(("", &out));
    let ok = serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| v["ok"].as_bool()).unwrap_or(false);
    if ok { Ok(json.to_string()) } else { Err(std::io::Error::other(json.to_string())) }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(std::env::var("ADG_POWER_SUPPLY_ROOT").unwrap_or_else(|_| battery::DEFAULT_SYSFS_ROOT.into()));
    match args.first().map(String::as_str) {
        Some("once") => print_once(&root, args.iter().any(|a| a == "--json")),
        Some("watch") => loop {
            print_once(&root, false);
            std::thread::sleep(daemon::SAMPLE_INTERVAL);
        },
        Some("call") if args.len() >= 3 => {
            let socket = server::default_socket();
            match call(&socket, &args[1], &args[2], args.get(3).map(String::as_str)) {
                Ok(body) => println!("{body}"),
                Err(e) => {
                    eprintln!("adg-powerd call: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("policy") if args.get(1).map(String::as_str) == Some("--generator-list") => {
            let scope = match args.get(2).map(String::as_str) {
                Some("user") => policy::Scope::User,
                _ => policy::Scope::System,
            };
            for u in policy::generator_list(scope) {
                println!("{u}");
            }
        }
        None | Some("serve") => {
            let socket = args
                .iter()
                .position(|a| a == "--socket")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
                .unwrap_or_else(server::default_socket);
            let d = Arc::new(daemon::Daemon::new(root));
            let s = d.clone();
            std::thread::spawn(move || loop {
                s.sample();
                std::thread::sleep(daemon::SAMPLE_INTERVAL);
            });
            if let Err(e) = server::serve(d, socket) {
                eprintln!("adg-powerd: {e}");
                std::process::exit(1);
            }
        }
        _ => usage(),
    }
}
