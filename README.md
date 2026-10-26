# Omarchy Mihomo

Mihomo profiles, subscriptions, proxy/group editors, routing rules, connection diagnostics and logs, backed by a shared Rust daemon.

- **Linux / Omarchy:** the existing QML panel in `plugin/` and systemd installer in `packaging/install.sh`.
- **macOS / Apple Silicon:** a native SwiftUI application, user daemon and privileged network helper. See [macOS build, install and verification](docs/macos.md).

## Autofix and notifications (Omarchy)

When a host keeps failing (3 failures in 5 minutes by default, none successful), the daemon checks it through the profile's groups. A working route goes on trial as a temporary rule: it is kept if the host keeps working and removed if it does not. Only traffic nobody directed is fixed — the final `Match`, or DIRECT by default: anything an explicit rule sent to a group (yours, by domain, process, path or IP, or the subscription's) is left alone, as are local addresses, DNS failures and the hosts and processes (`process:name`) you list. Strict groups (a work VPN) never lose traffic to another group and never receive it; when one goes down you only get a notification. Before a trial rule is kept, the original route is tried again and the rule is dropped if it works. Settings → Automation chooses between showing, suggesting and trying; Settings → Notifications chooses which events notify.

Omarchy notifications have no buttons; clicking one opens a small panel for that event with the actions.

A rule for the focused window's program, bound to a key in `~/.config/hypr/bindings.conf`:

```
bindd = SUPER ALT, P, Mihomo rule for window, exec, omm rule-for-window
```

## Sleep and network changes

Every 3 seconds the daemon looks at the default routes outside tunnels and at the suspend clock. While the machine is offline, and for 30 seconds after it wakes up or joins another network, failed connections are not recorded as problems, autofix neither counts them nor decides trials, and dead-group notices wait. Five seconds after the change it closes the core's connections (only for its own core) and re-tests the url-test, fallback and load-balance groups. A trial rule measures time awake: a sleep moves its end by as long as the machine slept. The panel footer shows "offline" or "network changed" meanwhile.

## Core and database updates (Omarchy)

Once a day the daemon asks GitHub for the newest stable mihomo (directly, then through the core) and, when it is newer than the running core, shows it in Settings → Core, among the setup issues and once per release as a notification. It never replaces the core on its own. With TUN capabilities on the binary the update runs `omm fix update_core` in a terminal: the download is verified against the release's SHA-256, smoke-tested, installed, given its capabilities back with sudo and restarted. The replaced binary stays as `mihomo.prev`, a hard link that keeps its capabilities; Settings → Core or `omm core rollback` puts it back.

The core refreshes GeoIP, GeoSite and the MMDBs itself (`geo-auto-update`, every 24 hours by default, Settings → Core); "Update now" asks it to fetch them at once. Traffic statistics re-read the databases when they change.

## Traffic statistics (Omarchy)

The daemon polls the core's open connections every 2 seconds and records how much each one downloaded and uploaded, by app, host, domain, address, provider (ASN), country, group, node and rule. Minutes are kept for two days and hours for 30 days in `traffic.db` in the data directory. Countries come from the core's `Country.mmdb` or, failing that, a copy of MetaCubeX `country.mmdb` the daemon downloads once; providers come from the core's `ASN.mmdb`. The Traffic page charts it over time with filters: click a row to narrow down to it (app → domains → hosts → addresses), `x` excludes it, `r` makes a rule, dragging over the chart zooms in. Settings → Main turns recording off or clears it.

Connections the core cannot name (other users' programs: pacman's `alpm`, root services) are attributed through `/proc/net` to the program, or to the user when its processes are private. The Rules page and the group cards show how much went through each over the last week. Every 5 minutes the daemon looks for an app that moved at least 512 MiB in an hour and five times its usual, or that went direct and now sends 100 MiB an hour through a proxy, and notifies once per app in 6 hours (Settings → Notifications → Unusual traffic).

## Development

```sh
cargo test --locked --workspace
mkdir -p target/qml-shots
QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tests/qml -import tests/qml/stubs
```

Shared business logic lives in the existing Rust crates. `omm-platform` owns OS paths, Darwin process enumeration and the private helper transport. `omm-helper` provides the macOS network service. GUI clients use the existing version-1 NDJSON protocol over a private Unix socket.

The macOS application and helper are built on a Mac with Xcode and Rust. CI builds Linux and macOS, tests Rust and the Swift transport, and uploads a local ad-hoc-signed application. Live TUN, network restoration and sleep/wake acceptance require a real Mac; compilation alone does not certify those scenarios.
