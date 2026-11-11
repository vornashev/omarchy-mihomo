# Development

```sh
cargo clippy --locked --workspace --all-targets -- -D warnings
omm core install --dest /tmp/mihomo && OMM_TEST_MIHOMO=/tmp/mihomo cargo test --locked --workspace
scripts/qml-check.sh
```

The real-core tests skip without `OMM_TEST_MIHOMO`, except on CI, where they fail. `scripts/qml-check.sh` runs the QML tests offscreen, fails on QML warnings from the plugin's own files (a `ReferenceError` does not fail a QML test by itself) and runs `qmllint` with the categories the `qs.*` stubs can check.

UI strings live in `plugin/i18n/ru.js` and `en.js`; `tst_i18n.qml` fails when a key is missing from either, defined twice, has different placeholders, or is used in the plugin without a translation.

Shared business logic lives in the existing Rust crates. `omm-platform` owns OS paths, Darwin process enumeration and the private helper transport. `omm-helper` provides the macOS network service. GUI clients use the existing version-1 NDJSON protocol over a private Unix socket.

The macOS application and helper are built on a Mac with Xcode and Rust. CI builds Linux and macOS, tests Rust and the Swift transport, and uploads a local ad-hoc-signed application. Live TUN, network restoration and sleep/wake acceptance require a real Mac; compilation alone does not certify those scenarios.

While working on the QML, `packaging/dev-plugin.sh --watch` copies `plugin/` and `manifest.json` into `~/.config/omarchy/plugins/vornashev.omarchy-mihomo` and restarts the shell on every change.
