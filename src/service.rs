use daemon_kit::{Daemon, DaemonConfig};

/// Create a daemon handle for the lambdaattack service.
pub fn daemon() -> Daemon {
    let config = DaemonConfig::new("lambdaattack")
        .description("LambdaAttack — Minecraft stress-test bot")
        .service_args(vec!["service".into(), "run".into()]);
    Daemon::new(config)
}

/// Print human-readable status.
pub fn print_status(daemon: &Daemon) {
    if daemon.is_running() {
        if let Some(pid) = daemon.running_pid() {
            println!("● lambdaattack.service — running (PID {pid})");
        } else {
            println!("● lambdaattack.service — running");
        }
    } else {
        if daemon.is_service_installed() {
            println!("○ lambdaattack.service — installed, not running");
        } else {
            println!("○ lambdaattack.service — not installed");
        }
    }
}
