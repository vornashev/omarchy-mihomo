//! Desktop proxy settings for Omarchy/Hyprland (the GSettings backend used
//! by Clash Verge on non-KDE Linux). No shell or privileged helper involved.
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use omm_proto::SystemProxyStatus;
use serde::{Deserialize, Serialize};

// Mode is written last, after all endpoints are ready.
const KEYS: &[(&str, &str)] = &[
    ("org.gnome.system.proxy.http", "host"),
    ("org.gnome.system.proxy.http", "port"),
    ("org.gnome.system.proxy.https", "host"),
    ("org.gnome.system.proxy.https", "port"),
    ("org.gnome.system.proxy.socks", "host"),
    ("org.gnome.system.proxy.socks", "port"),
    ("org.gnome.system.proxy.http", "enabled"),
    ("org.gnome.system.proxy.http", "use-authentication"),
    ("org.gnome.system.proxy", "use-same-proxy"),
    ("org.gnome.system.proxy", "ignore-hosts"),
    ("org.gnome.system.proxy", "mode"),
];
const MODE: usize = KEYS.len() - 1;
type Snapshot = Vec<String>;

trait Backend {
    async fn get(&self, index: usize) -> Result<String>;
    async fn set(&self, index: usize, value: &str) -> Result<()>;
}

pub struct GSettings;
impl GSettings {
    async fn run(args: &[&str]) -> Result<String> {
        let mut command = tokio::process::Command::new("gsettings");
        command.args(args).kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(3), command.output())
            .await
            .context("gsettings timed out")?
            .context("Cannot run gsettings; install glib2 and gsettings-desktop-schemas")?;
        // dconf may report a failed write on stderr while exiting with 0.
        if !output.status.success() || !output.stderr.is_empty() {
            bail!("gsettings: {}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
}
impl Backend for GSettings {
    async fn get(&self, index: usize) -> Result<String> {
        let (schema, key) = KEYS[index];
        Self::run(&["get", schema, key]).await
    }
    async fn set(&self, index: usize, value: &str) -> Result<()> {
        let (schema, key) = KEYS[index];
        Self::run(&["set", schema, key, value]).await?;
        Ok(())
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Saved {
    wanted: bool,
    previous: Option<Snapshot>,
    applied: Option<Snapshot>,
    #[serde(default)]
    recovery: Option<Snapshot>,
}

pub struct Manager<B = GSettings> {
    backend: B,
    path: PathBuf,
    saved: Saved,
}

impl Manager {
    pub fn open(path: PathBuf) -> Result<Self> {
        Self::with_backend(path, GSettings)
    }
}

#[allow(private_bounds)]
impl<B: Backend> Manager<B> {
    fn with_backend(path: PathBuf, backend: B) -> Result<Self> {
        let saved: Saved = match omm_config::paths::read_optional(&path)? {
            Some(text) => serde_json::from_str(&text).context("Invalid system proxy state")?,
            None => Saved::default(),
        };
        for snapshot in [&saved.previous, &saved.applied, &saved.recovery].into_iter().flatten() {
            if snapshot.len() != KEYS.len() {
                bail!("Invalid system proxy snapshot");
            }
        }
        Ok(Self { backend, path, saved })
    }

    fn save(&self) -> Result<()> {
        omm_config::paths::write_atomic(&self.path, &serde_json::to_vec(&self.saved)?)?;
        Ok(())
    }

    async fn read(&self) -> Result<Snapshot> {
        let mut values = Vec::with_capacity(KEYS.len());
        for index in 0..KEYS.len() {
            values.push(self.backend.get(index).await?);
        }
        Ok(values)
    }

    async fn write(&self, values: &Snapshot) -> Result<()> {
        self.backend.set(MODE, "'none'").await?;
        for (index, value) in values.iter().enumerate() {
            self.backend.set(index, value).await?;
        }
        if self.read().await? != *values {
            bail!("System proxy settings did not persist");
        }
        Ok(())
    }

    // Save recovery data before the first system mutation. On failure restore
    // both the desktop snapshot and the previous intent; keep recovery data
    // if rollback itself failed.
    async fn install(&mut self, current: Snapshot, target: Snapshot) -> Result<()> {
        let old = self.saved.clone();
        if self.saved.previous.is_none() {
            let mut previous = current.clone();
            // An already-active proxy for this same core must turn off when
            // released, including after migration from another client.
            if matches_port(&previous, target[1].parse().unwrap_or(0)) {
                previous[MODE] = "'none'".into();
            }
            self.saved.previous = Some(previous);
        }
        self.saved.applied = Some(target.clone());
        self.saved.recovery = self.saved.previous.clone();
        if let Err(error) = self.save() {
            self.saved = old;
            return Err(error);
        }
        if let Err(error) = self.write(&target).await {
            if let Err(rollback) = self.write(&current).await {
                bail!("{error}; restoring previous proxy failed: {rollback}");
            }
            self.saved = old;
            self.save()?;
            return Err(error);
        }
        self.saved.recovery = None;
        self.save()?;
        Ok(())
    }

    async fn recover(&mut self) -> Result<()> {
        if let Some(snapshot) = self.saved.recovery.clone() {
            self.write(&snapshot).await.context("Recovering interrupted system proxy change")?;
            self.saved.recovery = None;
            self.saved.previous = None;
            self.saved.applied = None;
            self.save()?;
        }
        Ok(())
    }

    async fn restore(&mut self, current: &Snapshot) -> Result<()> {
        if let Some(applied) = &self.saved.applied {
            // Another application/user changed the proxy: leave their settings.
            if current == applied
                && let Some(previous) = self.saved.previous.clone() {
                    self.saved.recovery = Some(previous.clone());
                    self.save()?;
                    self.write(&previous).await?;
                }
            self.saved.previous = None;
            self.saved.applied = None;
            self.saved.recovery = None;
            self.save()?;
        }
        Ok(())
    }

    pub async fn set(&mut self, enable: bool, port: u16) -> Result<SystemProxyStatus> {
        if enable && port == 0 {
            bail!("Start the mihomo core with a non-zero mixed-port first");
        }
        self.recover().await?;
        let current = self.read().await?;
        let old = self.saved.clone();
        if enable {
            if self.saved.applied.as_ref().is_some_and(|s| s != &current) {
                self.saved.previous = None;
                self.saved.applied = None;
            }
            self.saved.wanted = true;
            if let Err(error) = self.install(current, target(port)).await {
                // Preserve any recovery snapshots from a failed rollback.
                self.saved.wanted = old.wanted;
                self.save()?;
                return Err(error);
            }
        } else {
            self.saved.wanted = false;
            if let Err(error) = self.save() {
                self.saved.wanted = old.wanted;
                return Err(error);
            }
            self.restore(&current).await?;
            // Also allow switching off an existing local proxy adopted from
            // another client when this daemon has no restoration snapshot.
            if old.applied.is_none() && matches_port(&current, port) {
                self.backend.set(MODE, "'none'").await?;
            }
        }
        self.inspect(port).await
    }

    async fn inspect(&self, port: u16) -> Result<SystemProxyStatus> {
        let current = self.read().await?;
        Ok(SystemProxyStatus { available: true, enabled: matches_port(&current, port), wanted: self.saved.wanted, port, error: None })
    }

    pub async fn refresh(&mut self, port: u16) -> SystemProxyStatus {
        match self.sync(port).await {
            Ok(status) => status,
            Err(error) => SystemProxyStatus { wanted: self.saved.wanted, port, error: Some(error.to_string()), ..Default::default() },
        }
    }

    async fn sync(&mut self, port: u16) -> Result<SystemProxyStatus> {
        self.recover().await?;
        let current = self.read().await?;
        if port == 0 || !self.saved.wanted {
            self.restore(&current).await?;
        } else if self.saved.applied.as_ref().is_some_and(|s| s != &current) {
            // Do not fight changes made in desktop settings or another client.
            self.saved = Saved::default();
            self.save()?;
        } else if self.saved.applied.as_ref() != Some(&target(port)) {
            self.install(current, target(port)).await?;
        }
        self.inspect(port).await
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        // Keep the preference so a subsequent daemon/core start re-enables it.
        self.recover().await?;
        let current = self.read().await?;
        self.restore(&current).await
    }
}

fn target(port: u16) -> Snapshot {
    vec![
        "'127.0.0.1'".into(),
        port.to_string(),
        "'127.0.0.1'".into(),
        port.to_string(),
        "'127.0.0.1'".into(),
        port.to_string(),
        "true".into(),
        "false".into(),
        "false".into(),
        "['localhost', '127.0.0.1', '::1', '192.168.0.0/16', '10.0.0.0/8', '172.16.0.0/12']".into(),
        "'manual'".into(),
    ]
}

fn matches_port(values: &Snapshot, port: u16) -> bool {
    port != 0
        && values[MODE] == "'manual'"
        && [0, 2, 4].into_iter().all(|i| values[i] == "'127.0.0.1'" && values[i + 1].parse::<u16>().ok() == Some(port))
        && values[6] == "true"
        && values[7] == "false"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Fake {
        values: Arc<Mutex<Snapshot>>,
        fail_once: Arc<Mutex<Option<usize>>>,
        unavailable: bool,
    }
    impl Fake {
        fn new() -> Self {
            let mut values = target(3128);
            values[0] = "'office.proxy'".into();
            values[MODE] = "'auto'".into();
            Self { values: Arc::new(Mutex::new(values)), fail_once: Default::default(), unavailable: false }
        }
        fn snapshot(&self) -> Snapshot {
            self.values.lock().unwrap().clone()
        }
    }
    impl Backend for Fake {
        async fn get(&self, index: usize) -> Result<String> {
            if self.unavailable {
                bail!("No such schema");
            }
            Ok(self.values.lock().unwrap()[index].clone())
        }
        async fn set(&self, index: usize, value: &str) -> Result<()> {
            let mut fail = self.fail_once.lock().unwrap();
            if *fail == Some(index) {
                *fail = None;
                bail!("write rejected");
            }
            self.values.lock().unwrap()[index] = value.to_owned();
            Ok(())
        }
    }

    // Run explicitly with GSETTINGS_BACKEND=keyfile and XDG_CONFIG_HOME set
    // to a temporary directory. Never touches the user's dconf database.
    #[tokio::test]
    #[ignore = "requires isolated GSettings keyfile backend"]
    async fn gsettings_roundtrip() {
        assert_eq!(std::env::var("GSETTINGS_BACKEND").unwrap(), "keyfile");
        let config_home = PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap());
        assert!(config_home.starts_with(std::env::temp_dir()));
        let dir = tempfile::tempdir().unwrap();
        let mut manager = Manager::open(dir.path().join("proxy.json")).unwrap();
        let original = manager.read().await.unwrap();
        assert!(manager.set(true, 17897).await.unwrap().enabled);
        assert!(manager.refresh(17898).await.enabled);
        assert!(!manager.set(false, 17898).await.unwrap().enabled);
        assert_eq!(manager.read().await.unwrap(), original);
    }

    #[tokio::test]
    async fn enable_and_disable_restore_previous_desktop_settings() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        assert!(manager.set(true, 7897).await.unwrap().enabled);
        assert_eq!(fake.snapshot(), target(7897));
        assert!(!manager.set(false, 7897).await.unwrap().enabled);
        assert_eq!(fake.snapshot(), original);
        assert!(!manager.saved.wanted);
    }

    #[tokio::test]
    async fn follows_runtime_port_and_core_restart() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        manager.set(true, 7897).await.unwrap();
        assert!(manager.refresh(1080).await.enabled);
        assert_eq!(fake.snapshot(), target(1080));
        let stopped = manager.refresh(0).await;
        assert!(!stopped.enabled && stopped.wanted);
        assert_eq!(fake.snapshot(), original);
        assert!(manager.refresh(1080).await.enabled);
        manager.shutdown().await.unwrap();
        assert_eq!(fake.snapshot(), original);
        assert!(manager.saved.wanted);
    }

    #[tokio::test]
    async fn daemon_restart_keeps_intent_and_original_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxy.json");
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(path.clone(), fake.clone()).unwrap();
        manager.set(true, 7897).await.unwrap();
        drop(manager);
        let mut manager = Manager::with_backend(path.clone(), fake.clone()).unwrap();
        assert!(manager.refresh(7897).await.enabled);
        manager.shutdown().await.unwrap();
        assert_eq!(fake.snapshot(), original);
        let mut manager = Manager::with_backend(path, fake.clone()).unwrap();
        assert!(manager.refresh(7897).await.enabled);
        manager.set(false, 7897).await.unwrap();
        assert_eq!(fake.snapshot(), original);
    }

    #[tokio::test]
    async fn failed_enable_rolls_back_and_does_not_save_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let original = fake.snapshot();
        *fake.fail_once.lock().unwrap() = Some(3);
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        assert!(manager.set(true, 7897).await.is_err());
        assert_eq!(fake.snapshot(), original);
        assert!(!manager.saved.wanted);
        assert!(!manager.refresh(7897).await.enabled);
    }

    #[tokio::test]
    async fn restore_failure_is_recoverable_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxy.json");
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(path.clone(), fake.clone()).unwrap();
        manager.set(true, 7897).await.unwrap();
        *fake.fail_once.lock().unwrap() = Some(3);
        assert!(manager.shutdown().await.is_err());
        let mut manager = Manager::with_backend(path, fake.clone()).unwrap();
        manager.shutdown().await.unwrap();
        assert_eq!(fake.snapshot(), original);
    }

    #[tokio::test]
    async fn external_changes_are_reported_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        manager.set(true, 7897).await.unwrap();
        *fake.values.lock().unwrap() = target(8888);
        let status = manager.refresh(7897).await;
        assert!(!status.enabled && !status.wanted);
        manager.shutdown().await.unwrap();
        assert_eq!(fake.snapshot(), target(8888));
    }

    #[tokio::test]
    async fn unavailable_backend_and_stopped_core_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake { unavailable: true, ..Fake::new() };
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake).unwrap();
        let status = manager.refresh(7897).await;
        assert!(!status.available && status.error.is_some());
        assert!(manager.set(true, 7897).await.is_err());
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        assert!(manager.set(true, 0).await.is_err());
        assert_eq!(fake.snapshot(), original);
    }

    #[tokio::test]
    async fn failed_disable_does_not_reenable_during_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        let original = fake.snapshot();
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        manager.set(true, 7897).await.unwrap();
        *fake.fail_once.lock().unwrap() = Some(3);
        assert!(manager.set(false, 7897).await.is_err());
        let status = manager.refresh(7897).await;
        assert!(!status.enabled && !status.wanted && status.error.is_none());
        assert_eq!(fake.snapshot(), original);
    }

    #[tokio::test]
    async fn adopting_an_existing_proxy_can_still_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        *fake.values.lock().unwrap() = target(7897);
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        assert!(!manager.set(false, 7897).await.unwrap().enabled);
        *fake.values.lock().unwrap() = target(7897);
        assert!(manager.set(true, 7897).await.unwrap().enabled);
        assert!(!manager.set(false, 7897).await.unwrap().enabled);
        assert_eq!(fake.snapshot()[MODE], "'none'");
    }

    #[tokio::test]
    async fn wrong_endpoint_is_not_shown_as_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new();
        *fake.values.lock().unwrap() = target(7897);
        let mut manager = Manager::with_backend(dir.path().join("proxy.json"), fake.clone()).unwrap();
        assert!(manager.refresh(7897).await.enabled);
        fake.values.lock().unwrap()[4] = "'another.proxy'".into();
        assert!(!manager.refresh(7897).await.enabled);
    }
}
