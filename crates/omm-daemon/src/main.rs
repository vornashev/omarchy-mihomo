mod config;
mod core;
mod ipc;
mod methods;
mod state;

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
    let core_socket = args.core_socket.clone().unwrap_or_else(|| paths.controller_socket());
    let binary = omm_config::runtime::find_binary(args.core_bin.as_deref());
    if binary.is_none() {
        tracing::warn!("no mihomo binary found; configs cannot be validated or applied");
    }
    let manager = Manager::open(paths, binary)?;
    let state = Arc::new(State::new(mihomo_api::Client::new(&core_socket), manager));
    tracing::info!(core = %core_socket.display(), data = %args.data_dir.display(), "starting");

    let listener = ipc::bind(&args.socket)?;
    tokio::spawn(core::watch(state.clone()));
    tokio::spawn(core::pump_traffic(state.clone()));
    {
        let state = state.clone();
        tokio::spawn(async move {
            state.config.init(&state).await;
            config::auto_update(state).await;
        });
    }
    let server = tokio::spawn(ipc::serve(listener, state.clone()));

    shutdown_signal().await;
    tracing::info!("shutting down");
    server.abort();
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
