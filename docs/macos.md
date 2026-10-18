# macOS / Apple Silicon

Target: M1 and newer, macOS 14+. The requested acceptance machine is an M1 MacBook running macOS 26 Tahoe. This is a local installation, without Developer ID notarization or App Store distribution.

## Build and install

Install Xcode 15.3 or newer (including its command-line tools), Rust stable and Python 3. Run as your normal macOS user from the repository:

```sh
swift test --package-path apps/macos
packaging/macos/build.sh
packaging/macos/install.sh
open /Applications/Mihomo.app
```

The installer requests `sudo` for the network service. The UI and daemon run as your user. Import a YAML profile or subscription, connect, and enable system proxy or TUN as needed. Both network modes start disabled on a new installation. Login launch is opt-in in Settings. Closing the window keeps the menu item; Quit ends the network session. The next launch restores your explicit connection preference.

`~/.local/bin/omm` provides the existing CLI. `omm core start` opens a daemon-owned session, useful for diagnosis without the UI; `omm core stop` ends it. The application takes ownership when it attaches, and exiting it stops that session.

Repeat build and install for updates. Quit the app first. One previous `.app` is retained as `/Applications/Mihomo.previous.app`; to roll back, build/install the previous Git revision so the application and helper stay in sync. Automatic core upgrades are disabled on macOS: installed root executables are replaced only by the administrator installer.

```sh
packaging/macos/uninstall.sh
```

Removal restores proxy settings and preserves profiles, state and recovery logs. The installer supports one owning user per Mac; uninstall before changing the owner.

## Layout and privilege boundary

- UI: `/Applications/Mihomo.app`; user data: `~/Library/Application Support/omarchy-mihomo`; user logs: `~/Library/Logs/omarchy-mihomo`.
- User daemon socket: `~/.omm/daemon.sock` (private directory, mode 0600 socket). Explicit `OMM_SOCKET` / `OMM_DATA_DIR` overrides remain available. Very long home paths need a shorter private socket override.
- Root tools: `/Library/PrivilegedHelperTools/omarchy-mihomo`; private core runtime and recovery journal: `/Library/Application Support/omarchy-mihomo/<uid>`.
- `launchd` labels: `io.github.vornashev.omarchy-mihomo.daemon` (user) and `io.github.vornashev.omarchy-mihomo.helper` (system).

The daemon holds a helper lease while a network session exists. Dropping it stops the core and restores network proxy settings. The helper checks peer UID, bounds request sizes, uses fixed executable paths and a private runtime, rejects unsafe file references and limits controller endpoints. Root binaries are never replaced through RPC. The helper controller relay permits reads, group selection, connection closure and a small set of runtime patches; configuration reloads pass through validation and rollback.

SystemConfiguration snapshots preserve HTTP, HTTPS, SOCKS, PAC and discovery settings per network service. Restoration only restores fields still equal to values the app wrote. A service changed externally is released rather than repeatedly overwritten. A recovery journal is saved before changing system preferences and replayed on helper startup.

Linux migrations, external executable proxy plugins, unsafe file paths, Linux-specific TUN options and Intel Macs are outside this version. Provider/certificate file references must exist in the managed core runtime; embedded PEM and HTTP providers avoid local file provisioning. The GUI exposes errors from these restrictions rather than claiming the profile applied.

## Protocol additions

Version 1 gains additive `platform`, `capabilities` and `helper` status fields. `session.open` attaches UI ownership to the connection; `session.start` / `session.stop` manage networking. A second UI owner gets `conflict`. UI disconnection aborts its pending daemon requests and releases its network session. CLI-only starts can persist until explicitly stopped or the daemon exits.

`profiles.read {id}` returns `{content}`. `profiles.save {id, expected, content}` validates the YAML/core config, refuses stale content, writes atomically and applies the selected profile. Other screens use the existing proxy, group, rule, problem and connection RPCs. New error codes include `unsupported`, `helper_unavailable` and `permission_denied`; existing QML clients ignore additive status fields.

## Acceptance on a real Mac

Run these before treating the port as production-ready:

1. Fresh install without a profile: no proxy settings or routes change. Import a profile, connect, switch proxy groups and verify TCP and UDP traffic.
2. Enable/disable TUN; verify DNS, domain rules and process-name/path rules. Test IPv6 with an IPv6-enabled profile. A rejected config must preserve the last working configuration.
3. Start with a manual proxy or PAC; enable system proxy, quit and verify exact restoration. Edit a managed field externally and verify it is not overwritten. Switch Wi-Fi and attach a second network adapter.
4. Sleep/wake, loss of connectivity, network change and reboot: verify traffic resumes when connected, or the UI reports an actionable error. No loop or stale system proxy should remain after stopping.
5. Kill the UI, daemon and helper separately; verify service restart and journal recovery. Kill Mihomo and verify proxy restoration and a visible error. Test a connection from another UID and unsafe provider paths.
6. Exercise profiles/subscriptions, full node/group forms and YAML mode, import previews, rule suggestions/testing/history, problem hiding, connection closure and live logs in Russian/English and light/dark appearances.
7. Update the app while disconnected, then uninstall while connected; verify network recovery and preservation of user data.

Tests in CI avoid changing the runner's routes or system network preferences. Live networking and visual QA remain explicit acceptance steps.
