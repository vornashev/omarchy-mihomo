#!/bin/bash
# Copy the shell plugin into omarchy-shell's plugin directory and restart the
# shell: its own reload keeps components such as ProblemTableRow cached.
# Run with --watch to keep syncing.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
dest="$HOME/.config/omarchy/plugins/vornashev.omarchy-mihomo"

sync() {
  mkdir -p "$dest/plugin"
  cp "$repo/manifest.json" "$dest/manifest.json"
  rsync -a --delete "$repo/plugin/" "$dest/plugin/"
  if command -v omarchy-restart-shell >/dev/null; then omarchy-restart-shell; fi
}

sync
echo "synced to $dest"
if [[ "${1:-}" == "--watch" ]]; then
  while inotifywait -qq -r -e close_write,create,delete,move "$repo/plugin" "$repo/manifest.json"; do
    sync
    echo "synced $(date +%T)"
  done
fi
