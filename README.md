# Omarchy Mihomo

Mihomo profiles, subscriptions, proxy/group editors, routing rules, connection diagnostics and logs, backed by a shared Rust daemon.

- **Linux / Omarchy:** the existing QML panel in `plugin/` and systemd installer in `packaging/install.sh`.
- **macOS / Apple Silicon:** a native SwiftUI application, user daemon and privileged network helper. See [macOS build, install and verification](docs/macos.md).

## Development

```sh
cargo test --locked --workspace
QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tests/qml -import tests/qml/stubs
```

Shared business logic lives in the existing Rust crates. `omm-platform` owns OS paths, Darwin process enumeration and the private helper transport. `omm-helper` provides the macOS network service. GUI clients use the existing version-1 NDJSON protocol over a private Unix socket.

The macOS application and helper are built on a Mac with Xcode and Rust. CI builds Linux and macOS, tests Rust and the Swift transport, and uploads a local ad-hoc-signed application. Live TUN, network restoration and sleep/wake acceptance require a real Mac; compilation alone does not certify those scenarios.
