//! Installed root service. Only the installed user's UID may call it.
mod config;
mod relay;
use anyhow::{Context, Result, bail};
use clap::Parser;
use omm_platform::helper::{VERSION, read_frame};
use serde_json::{Value, json};
use std::{
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::{Child, Command},
    sync::Mutex,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    uid: u32,
}
struct Runtime {
    child: Option<Child>,
    leased: bool,
    generation: u64,
    proxy_port: u16,
    error: Option<String>,
}
pub struct Service {
    user: u32,
    root: PathBuf,
    runtime: Mutex<Runtime>,
}
impl Service {
    fn core_home(&self) -> PathBuf {
        self.root.join("runtime")
    }
    fn controller(&self) -> PathBuf {
        self.core_home().join("controller.sock")
    }
    async fn proxy(&self, mode: &str, port: u16) -> Result<Value> {
        let out = tokio::time::timeout(
            Duration::from_secs(10),
            Command::new(format!("{}/omm-system-proxy", omm_platform::INSTALL_ROOT))
                .args([
                    mode,
                    &port.to_string(),
                    self.root.join("proxy-state.json").to_str().unwrap(),
                ])
                .env_clear()
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        if !out.status.success() {
            bail!(
                "system proxy: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(serde_json::from_slice(&out.stdout)?)
    }
    async fn stop(&self, runtime: &mut Runtime) -> Result<()> {
        let restore = self.proxy("restore", 0).await;
        runtime.proxy_port = 0;
        if let Some(mut child) = runtime.child.take() {
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            if tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .is_err()
            {
                child.kill().await?;
            }
        }
        restore?;
        Ok(())
    }
    async fn install_config(&self, yaml: &str) -> Result<()> {
        let yaml = config::prepare(yaml)?;
        let home = self.core_home();
        assert_private_tree(&home)?;
        let staged = home.join("candidate.yaml");
        atomic(&staged, yaml.as_bytes())?;
        let check = tokio::time::timeout(
            Duration::from_secs(60),
            core_command(&home)
                .args(["-t", "-f", "candidate.yaml"])
                .output(),
        )
        .await??;
        if !check.status.success() {
            bail!(
                "Mihomo rejected the configuration: {}",
                String::from_utf8_lossy(&check.stdout)
                    .lines()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        if home.join("config.yaml").exists() {
            std::fs::copy(home.join("config.yaml"), home.join("previous.yaml"))?;
        }
        std::fs::rename(staged, home.join("config.yaml"))?;
        Ok(())
    }
    async fn request(&self, method: &str, p: Value) -> Result<Value> {
        let mut runtime = self.runtime.lock().await;
        match method {
            "status" => Ok(
                json!({"version":VERSION,"running":runtime.child.is_some(),"leased":runtime.leased,"error":runtime.error}),
            ),
            "core.stop" => {
                runtime.leased = false;
                runtime.generation += 1;
                self.stop(&mut runtime).await?;
                Ok(json!({"ok":true}))
            }
            "proxy.set" | "proxy.sync" => {
                let enable = p["enable"].as_bool().context("enable must be boolean")?;
                let port = p["port"].as_u64().filter(|p| *p <= 65535).unwrap_or(0) as u16;
                if enable && (!runtime.leased || runtime.child.is_none() || port == 0) {
                    bail!("Start a network session first");
                }
                let value = self
                    .proxy(
                        if enable {
                            if method == "proxy.sync" {
                                "sync"
                            } else {
                                "enable"
                            }
                        } else {
                            "restore"
                        },
                        port,
                    )
                    .await?;
                runtime.proxy_port = if value["enabled"] == true { port } else { 0 };
                Ok(value)
            }
            "core.start" | "core.reload" => {
                if !runtime.leased {
                    bail!("Acquire a session lease first");
                }
                if method == "core.start" && runtime.child.is_some() {
                    return Ok(json!({"ok":true}));
                }
                self.install_config(p["yaml"].as_str().context("yaml is required")?)
                    .await?;
                if runtime.child.is_some() {
                    if let Err(e) = relay::reload(&self.controller()).await {
                        let home = self.core_home();
                        if home.join("previous.yaml").is_file() {
                            std::fs::copy(home.join("previous.yaml"), home.join("config.yaml"))?;
                            let _ = relay::reload(&self.controller()).await;
                        }
                        return Err(e);
                    }
                } else {
                    let _ = std::fs::remove_file(self.controller());
                    // This log is private and rotated at every start; live logs use the API.
                    let log_path = self.root.join("core.log");
                    if log_path.is_file() {
                        std::fs::rename(&log_path, self.root.join("core.previous.log"))?;
                    }
                    let log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .mode(0o600)
                        .open(log_path)?;
                    runtime.child = Some(
                        core_command(&self.core_home())
                            .args(["-f", "config.yaml"])
                            .stdout(log.try_clone()?)
                            .stderr(log)
                            .spawn()?,
                    );
                    let ready = async {
                        for _ in 0..100 {
                            if UnixStream::connect(self.controller()).await.is_ok() {
                                return Ok(());
                            }
                            if runtime.child.as_mut().unwrap().try_wait()?.is_some() {
                                bail!("Mihomo exited during startup; inspect the private core log");
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        bail!("Mihomo controller did not become ready")
                    }
                    .await;
                    if let Err(e) = ready {
                        let _ = self.stop(&mut runtime).await;
                        return Err(e);
                    }
                }
                runtime.error = None;
                Ok(json!({"ok":true}))
            }
            _ => bail!("unsupported helper operation"),
        }
    }
}
fn core_command(home: &Path) -> Command {
    let mut cmd = Command::new(format!("{}/mihomo", omm_platform::INSTALL_ROOT));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", home)
        .current_dir(home)
        .arg("-d")
        .arg(home)
        .kill_on_drop(true);
    cmd
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let temp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(temp, path)?;
    Ok(())
}
fn assert_private_tree(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        bail!("unsafe runtime ownership or symlink: {}", path.display());
    }
    if metadata.is_dir() {
        for item in std::fs::read_dir(path)? {
            assert_private_tree(&item?.path())?;
        }
    }
    Ok(())
}
fn socket(path: &Path, user: u32) -> Result<UnixListener> {
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            bail!("another helper is running");
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    if unsafe { libc::chown(name.as_ptr(), user, u32::MAX) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(listener)
}
async fn control(mut stream: UnixStream, service: Arc<Service>) -> Result<()> {
    if stream.peer_cred()?.uid() != service.user {
        bail!("unauthorized UID");
    }
    let request = tokio::time::timeout(
        Duration::from_secs(5),
        read_frame(&mut BufReader::new(&mut stream)),
    )
    .await??
    .context("empty request")?;
    if request["version"] != VERSION {
        bail!("helper protocol mismatch");
    }
    let method = request["method"].as_str().context("missing method")?;
    if method == "lease" {
        let mut runtime = service.runtime.lock().await;
        if runtime.leased {
            stream
                .write_all(b"{\"error\":\"another daemon owns the network session\"}\n")
                .await?;
            return Ok(());
        }
        runtime.leased = true;
        runtime.generation += 1;
        let generation = runtime.generation;
        drop(runtime);
        // Write failure must also release ownership.
        if stream
            .write_all(b"{\"result\":{\"ok\":true}}\n")
            .await
            .is_ok()
        {
            let _ = stream.read(&mut [0u8; 1]).await;
        }
        let mut runtime = service.runtime.lock().await;
        if runtime.generation == generation {
            runtime.leased = false;
            if let Err(e) = service.stop(&mut runtime).await {
                runtime.error = Some(e.to_string());
            }
        }
        return Ok(());
    }
    let result = service.request(method, request["params"].clone()).await;
    let response = match result {
        Ok(value) => json!({"result":value}),
        Err(e) => json!({"error":e.to_string()}),
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    if !omm_platform::IS_MACOS || omm_platform::uid() != 0 || args.uid == 0 {
        bail!("This service must be installed as root on macOS for a non-root user");
    }
    unsafe {
        libc::umask(0o077);
    }
    let root = PathBuf::from(format!(
        "/Library/Application Support/omarchy-mihomo/{}",
        args.uid
    ));
    std::fs::create_dir_all(root.join("runtime"))?;
    assert_private_tree(&root)?;
    let sockets = omm_platform::helper_directory(args.uid);
    std::fs::create_dir_all(&sockets)?;
    for dir in [sockets.parent().unwrap(), sockets.as_path()] {
        let m = std::fs::symlink_metadata(dir)?;
        if m.uid() != 0 || m.file_type().is_symlink() {
            bail!("unsafe socket directory");
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o711))?;
    }
    let listener = socket(&sockets.join("control.sock"), args.uid)?;
    let api = socket(&sockets.join("api.sock"), args.uid)?;
    let service = Arc::new(Service {
        user: args.uid,
        root,
        runtime: Mutex::new(Runtime {
            child: None,
            leased: false,
            generation: 0,
            proxy_port: 0,
            error: None,
        }),
    });
    // Recover a journal left by a previous helper before accepting requests.
    service.proxy("restore", 0).await?;
    let api_service = service.clone();
    tokio::spawn(async move {
        relay::serve(api, api_service).await;
    });
    let watcher = service.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let mut runtime = watcher.runtime.lock().await;
            if runtime
                .child
                .as_mut()
                .is_some_and(|c| c.try_wait().ok().flatten().is_some())
            {
                let _ = watcher.stop(&mut runtime).await;
                runtime.error =
                    Some("Mihomo stopped unexpectedly. Start the session again.".into());
            } else if runtime.proxy_port != 0 {
                if let Err(e) = watcher.proxy("sync", runtime.proxy_port).await {
                    runtime.error = Some(e.to_string());
                    let _ = watcher.stop(&mut runtime).await;
                }
            }
        }
    });
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        tokio::select! {
            _ = term.recv() => break,
            _ = tokio::signal::ctrl_c() => break,
            connection = listener.accept() => {
                let (stream,_) = connection?; let service = service.clone();
                tokio::spawn(async move { if let Err(e) = control(stream,service).await { eprintln!("helper connection: {e}"); } });
            }
        }
    }
    service.stop(&mut *service.runtime.lock().await).await?;
    Ok(())
}
