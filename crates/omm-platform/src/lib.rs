//! OS boundaries shared by the daemon, CLI and native clients.
use std::path::PathBuf;
pub mod helper;
#[cfg(target_os = "macos")]
pub mod macos;

pub const IS_MACOS: bool = cfg!(target_os = "macos");
pub const NAME: &str = std::env::consts::OS;
pub const INSTALL_ROOT: &str = "/Library/PrivilegedHelperTools/omarchy-mihomo";
pub const HELPER_LABEL: &str = "io.github.vornashev.omarchy-mihomo.helper";
pub const DAEMON_LABEL: &str = "io.github.vornashev.omarchy-mihomo.daemon";

pub fn uid() -> u32 { unsafe { libc::geteuid() } }
pub fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default() }
pub fn data_root() -> PathBuf {
    if IS_MACOS { home().join("Library/Application Support/omarchy-mihomo") }
    else { std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("omarchy-mihomo") }
}
pub fn socket_path() -> PathBuf {
    if IS_MACOS {
        // Stable across GUI/launchd environments, independent of TMPDIR and username length.
        home().join(".omm/daemon.sock")
    } else {
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("omarchy-mihomo/daemon.sock")
    }
}
pub fn core_binary() -> PathBuf {
    if IS_MACOS { PathBuf::from(INSTALL_ROOT).join("mihomo") }
    else { home().join(".local/lib/omarchy-mihomo/mihomo") }
}
pub fn helper_directory(user: u32) -> PathBuf { PathBuf::from(format!("/var/run/omarchy-mihomo/{user}")) }
pub fn helper_socket() -> PathBuf { helper_directory(uid()).join("control.sock") }
pub fn core_socket() -> PathBuf { helper_directory(uid()).join("api.sock") }
pub fn boot_id() -> String {
    #[cfg(target_os = "macos")]
    { macos::boot_id() }
    #[cfg(not(target_os = "macos"))]
    { std::fs::read_to_string("/proc/sys/kernel/random/boot_id").map(|s| s.trim().to_owned()).unwrap_or_default() }
}

/// Unix sockets have a much smaller path limit than ordinary files.
pub fn validate_socket(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let max = if IS_MACOS { 103 } else { 107 };
    if path.as_os_str().as_bytes().len() > max {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Unix socket path exceeds {max} bytes; set OMM_SOCKET to a shorter private path")));
    }
    Ok(())
}
