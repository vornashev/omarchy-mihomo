#!/bin/bash
# Run the current working tree: build and install omm + omarchy-mihomod,
# restart the daemon, copy the plugin and restart omarchy-shell.
# The mihomo core keeps running; the daemon reconnects to it.
#   --plugin-only   skip the Rust build and the daemon restart
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
bin_dir="$HOME/.local/bin"
say() { printf '\033[1m==>\033[0m %s\n' "$*"; }

plugin_only=0
for arg in "$@"; do
  case "$arg" in
    --plugin-only) plugin_only=1 ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [[ $plugin_only == 0 ]]; then
  # rustup's cargo: the mise shim may have no Rust version set.
  cargo="$HOME/.cargo/bin/cargo"
  [[ -x $cargo ]] || cargo=$(command -v cargo)

  say "building (release)"
  "$cargo" build --release --manifest-path "$repo/Cargo.toml" -p omm-daemon -p omm-cli

  say "installing binaries to $bin_dir"
  install -Dm755 "$repo/target/release/omm" "$bin_dir/omm"
  install -Dm755 "$repo/target/release/omarchy-mihomod" "$bin_dir/omarchy-mihomod"

  say "restarting the daemon"
  systemctl --user restart omarchy-mihomod.service
  for _ in {1..20}; do
    "$bin_dir/omm" status >/dev/null 2>&1 && break
    sleep 0.25
  done
  "$bin_dir/omm" status || echo "daemon not answering: journalctl --user -u omarchy-mihomod -n 30" >&2
fi

say "copying the plugin"
"$repo/packaging/dev-plugin.sh"

say "restarting omarchy-shell"
omarchy-restart-shell

say "done"
