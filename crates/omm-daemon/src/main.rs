mod config;
mod core;
mod geosite;
#[cfg_attr(target_os = "macos", path = "health_macos.rs")]
mod health;
mod ipc;
mod logs;
mod methods;
mod problems;
mod procs;
mod route_check;
mod seen;
mod state;
mod suggest;
#[cfg_attr(target_os = "macos", path = "system_proxy_macos.rs")]
mod system_proxy;
mod session;
mod user_rules;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use omm_config::paths::Paths;

use crate::config::Manager;
use crate::state::State;

#[derive(Debug, Parser)]
#[command(name = "omarchy-mihomod", version, about = "omarchy-mihomo daemon")]
struct Args {
    /// Socket the daemon listens on.
    #[arg(long, env = "OMM_SOCKET", default_value_os_t = omm_proto::default_socket_path())]
    socket: PathBuf,

    /// Data directory: profiles, overrides, settings, the core's home.
    #[arg(long, env = "OMM_DATA_DIR", default_value_os_t = Paths::default_root())]
    data_dir: PathBuf,

    /// mihomo controller socket. Defaults to our core's
    /// `<data-dir>/runtime/mihomo.sock`; pointing it elsewhere (e.g. at
    /// Clash Verge's core) makes the daemon observe that core read-only.
    #[arg(long, env = "OMM_CORE_SOCKET")]
    core_socket: Option<PathBuf>,

    /// mihomo binary used to validate configs.
    #[arg(long, env = "OMM_CORE_BIN")]
    core_bin: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OMM_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .without_time() // journald stamps lines already
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let args = Args::parse();
    let paths = Paths::new(&args.data_dir);
    paths.ensure()?;
    let core_socket = args.core_socket.clone().unwrap_or_else(|| paths.controller_socket());
    let binary = omm_config::runtime::find_binary(args.core_bin.as_deref());
    if binary.is_none() {
        tracing::warn!("no mihomo binary found; configs cannot be validated or applied");
    }
    let collector = omm_problems::Collector::open(&paths.root.join("state.db"))?;
    let system_proxy = system_proxy::Manager::open(paths.root.join("system-proxy.json"))?;
    let manager = Manager::open(paths, binary)?;
    let state = Arc::new(State::new(mihomo_api::Client::new(&core_socket), manager, collector, system_proxy));
    tracing::info!(core = %core_socket.display(), data = %args.data_dir.display(), "starting");

    let listener = ipc::bind(&args.socket)?;
    let core_watcher = tokio::spawn(core::watch(state.clone()));
    tokio::spawn(core::pump_traffic(state.clone()));
    tokio::spawn(config::watch_files(state.clone()));
    tokio::spawn(health::auto(state.clone()));
    let recorded = Arc::default();
    tokio::spawn(problems::collect(state.clone(), Arc::clone(&recorded)));
    tokio::spawn(problems::flush(state.clone(), recorded));
    tokio::spawn(config::expire_rules(state.clone()));
    {
        let state = state.clone();
        tokio::spawn(async move {
            state.config.init(&state).await;
            config::auto_update(state).await;
        });
    }
    let server = tokio::spawn(ipc::serve(listener, state.clone()));

    shutdown_signal().await;
    if omm_platform::IS_MACOS { let _ = session::stop(&state).await; }
    tracing::info!("shutting down");
    if let Err(e) = state.problems.lock().unwrap().flush() {
        tracing::warn!("saving problems: {e}");
    }
    server.abort();
    core_watcher.abort();
    if let Err(e) = state.system_proxy.lock().await.shutdown().await {
        tracing::warn!("restoring system proxy: {e}");
    }
    let _ = std::fs::remove_file(&args.socket);
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
