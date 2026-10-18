//! Facts about the machine around omarchy-mihomo: our systemd units, other
//! proxy clients, the core binary. Shared by the daemon's health checks and
//! `omm doctor`. Everything here is cheap and blocking.

use std::path::PathBuf;
use std::process::Command;

pub const CORE_UNIT: &str = "omarchy-mihomo-core.service";
pub const DAEMON_UNIT: &str = "omarchy-mihomod.service";
/// Clash Verge Rev's root service that runs its core.
pub const CVR_SERVICE: &str = "clash-verge-service.service";
/// Clash Verge Rev's GUI process.
pub const CVR_APP: &str = "clash-verge";
/// A user timer that rewrites Clash Verge's rule files on its own.
pub const AUTOTUNE_TIMER: &str = "site-autotune-tuner.timer";
/// site-autotune's log collector; useless once the timer is gone.
pub const AUTOTUNE_COLLECTOR: &str = "site-autotune-collector.service";

/// Cores of other clients that may hold the TUN device and routes, by
/// process name, with the client they belong to.
const OTHER_CORES: &[(&str, &str)] = &[("verge-mihomo", "Clash Verge")];

/// The client whose core is running, if any.
pub fn other_core() -> Option<&'static str> {
    if omm_platform::IS_MACOS { return OTHER_CORES.iter().find(|(name, _)| process_running(name)).map(|(_, app)| *app); }
    let procs = std::fs::read_dir("/proc").ok()?;
    procs.flatten().find_map(|e| {
        let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
        OTHER_CORES.iter().find(|(name, _)| comm.trim() == *name).map(|(_, app)| *app)
    })
}

pub fn process_running(name: &str) -> bool {
    Command::new("pgrep").args(["-x", name]).output().map(|o| o.status.success()).unwrap_or(false)
}

/// `systemctl [--user] <verb> <unit>` output, e.g. `active` or `enabled`.
pub fn unit_query(user: bool, verb: &str, unit: &str) -> String {
    let mut cmd = Command::new("systemctl");
    if user {
        cmd.arg("--user");
    }
    cmd.args([verb, unit]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

pub fn unit_active(user: bool, unit: &str) -> bool {
    unit_query(user, "is-active", unit) == "active"
}

/// Run `systemctl --user <args>`; the error carries systemctl's own words.
pub fn user_systemctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("systemctl").arg("--user").args(args).output().map_err(|e| format!("systemctl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(format!("systemctl --user {}: {err}", args.join(" ")))
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Clash Verge Rev's data directory, when it has profiles.
pub fn cvr_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"));
    let dir = base.join("io.github.clash-verge-rev.clash-verge-rev");
    dir.join("profiles.yaml").is_file().then_some(dir)
}

/// Clash Verge's login autostart entry.
pub fn cvr_autostart() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    base.join("autostart").join("Clash Verge.desktop")
}

/// Floating terminal Omarchy uses for install-like actions; sudo can ask
/// for a password there.
pub const TERMINAL_LAUNCHER: &str = "omarchy-launch-floating-terminal-with-presentation";
