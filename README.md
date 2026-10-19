# Omarchy Mihomo

Mihomo profiles, subscriptions, proxy/group editors, routing rules, connection diagnostics and logs, backed by a shared Rust daemon.

- **Linux / Omarchy:** the existing QML panel in `plugin/` and systemd installer in `packaging/install.sh`.
- **macOS / Apple Silicon:** a native SwiftUI application, user daemon and privileged network helper. See [macOS build, install and verification](docs/macos.md).

## Autofix and notifications (Omarchy)

When a host keeps failing (3 failures in 5 minutes by default, none successful), the daemon checks it through the profile's groups. A working route goes on trial as a temporary rule: it is kept if the host keeps working and removed if it does not. Hosts with your own rules, local addresses and DNS failures are left alone. Settings → Automation chooses between showing, suggesting and trying, and which events notify.

Omarchy notifications have no buttons; clicking one opens a small panel for that event with the actions.

A rule for the focused window's program, bound to a key in `~/.config/hypr/bindings.conf`:

```
bindd = SUPER ALT, P, Mihomo rule for window, exec, omm rule-for-window
```

## Development

```sh
cargo test --locked --workspace
mkdir -p target/qml-shots
QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tests/qml -import tests/qml/stubs
```

Shared business logic lives in the existing Rust crates. `omm-platform` owns OS paths, Darwin process enumeration and the private helper transport. `omm-helper` provides the macOS network service. GUI clients use the existing version-1 NDJSON protocol over a private Unix socket.

The macOS application and helper are built on a Mac with Xcode and Rust. CI builds Linux and macOS, tests Rust and the Swift transport, and uploads a local ad-hoc-signed application. Live TUN, network restoration and sleep/wake acceptance require a real Mac; compilation alone does not certify those scenarios.
