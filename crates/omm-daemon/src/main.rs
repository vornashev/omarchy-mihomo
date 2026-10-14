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

use crate::state::State;

#[derive(Debug, Parser)]
#[command(name = "omarchy-mihomod", version, about = "omarchy-mihomo daemon")]
struct Args {
    /// Socket the daemon listens on.
    #[arg(long, env = "OMM_SOCKET", default_value_os_t = omm_proto::default_socket_path())]
    socket: PathBuf,

    /// mihomo `external-controller-unix` socket.
    #[arg(long, env = "OMM_CORE_SOCKET", default_value_os_t = default_core_socket())]
    core_socket: PathBuf,
}

fn default_core_socket() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    data.join("omarchy-mihomo/runtime/mihomo.sock")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OMM_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .without_time() // journald stamps lines already
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let args = Args::parse();
    let state = Arc::new(State::new(mihomo_api::Client::new(&args.core_socket)));
    tracing::info!(core = %args.core_socket.display(), "starting");

    let listener = ipc::bind(&args.socket)?;
    tokio::spawn(core::watch(state.clone()));
    tokio::spawn(core::pump_traffic(state.clone()));
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
