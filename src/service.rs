use daemon_kit::{Daemon, DaemonConfig};

/// Create a daemon handle for the creeper service.
pub fn daemon() -> Daemon {
    let config = DaemonConfig::new("creeper")
        .description("Creeper — Minecraft stress-test bot")
        .service_args(vec!["service".into(), "run".into()]);
    Daemon::new(config)
}

/// Print human-readable status.
pub fn print_status(daemon: &Daemon) {
    if daemon.is_running() {
        if let Some(pid) = daemon.running_pid() {
            println!("● creeper.service — running (PID {pid})");
        } else {
            println!("● creeper.service — running");
        }
    } else {
        if daemon.is_service_installed() {
            println!("○ creeper.service — installed, not running");
        } else {
            println!("○ creeper.service — not installed");
        }
    }
}
