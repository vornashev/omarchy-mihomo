#!/bin/bash
# Install omarchy-mihomo for the current user:
#   binaries      ~/.local/bin/{omm,omarchy-mihomod}
#   core          ~/.local/lib/omarchy-mihomo/mihomo (pinned release, sha256-checked)
#   services      ~/.config/systemd/user/omarchy-mihomo{d,-core}.service
#   shell plugin  ~/.config/omarchy/plugins/tema.omarchy-mihomo
#
# The core is enabled but not started while Clash Verge Rev runs: two cores
# with TUN would fight over routes. `omm migrate cvr` does the switch.
#
# Options:
#   --yes          do not ask; skip steps that need sudo (setcap)
#   --setcap       grant TUN capabilities without asking (uses sudo)
#   --no-plugin    leave the bar alone
#   --upgrade-core reinstall the core binary even if present
#   --uninstall    stop and remove everything except data (add --purge for data)
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
bin_dir="$HOME/.local/bin"
unit_dir="$HOME/.config/systemd/user"
plugin_id="tema.omarchy-mihomo"
plugin_dir="$HOME/.config/omarchy/plugins/$plugin_id"
core_bin="$HOME/.local/lib/omarchy-mihomo/mihomo"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/omarchy-mihomo"

yes=0 setcap=ask plugin=1 upgrade_core=0 uninstall=0 purge=0
for arg in "$@"; do
  case "$arg" in
    --yes) yes=1 ;;
    --setcap) setcap=yes ;;
    --no-plugin) plugin=0 ;;
    --upgrade-core) upgrade_core=1 ;;
    --uninstall) uninstall=1 ;;
    --purge) purge=1 ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!!\033[0m %s\n' "$*"; }
ask() {
  [[ $yes == 1 ]] && return 1
  [[ -t 0 ]] || return 1
  read -r -p "$1 [y/N] " answer
  [[ $answer == [yY]* ]]
}

if [[ $uninstall == 1 ]]; then
  say "stopping services"
  systemctl --user disable --now omarchy-mihomo-core.service omarchy-mihomod.service 2>/dev/null || true
  rm -f "$unit_dir/omarchy-mihomo-core.service" "$unit_dir/omarchy-mihomod.service"
  systemctl --user daemon-reload
  rm -f "$bin_dir/omm" "$bin_dir/omarchy-mihomod" "$core_bin"
  if [[ -d $plugin_dir ]]; then
    say "removing the shell plugin"
    omarchy plugin disable "$plugin_id" >/dev/null 2>&1 || true
    rm -rf "$plugin_dir"
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  fi
  if [[ $purge == 1 ]]; then
    say "removing data in $data_dir"
    rm -rf "$data_dir"
  else
    echo "data kept in $data_dir (profiles, rules, problems); --purge removes it"
  fi
  exit 0
fi

cargo=$(command -v cargo 2>/dev/null || true)
[[ -x $HOME/.cargo/bin/cargo ]] && cargo="$HOME/.cargo/bin/cargo"
[[ -n $cargo ]] || { echo "cargo not found (install rustup)" >&2; exit 1; }

say "building (release)"
"$cargo" build --release --locked --manifest-path "$repo/Cargo.toml" -p omm-daemon -p omm-cli

say "installing binaries to $bin_dir"
install -Dm755 "$repo/target/release/omm" "$bin_dir/omm"
install -Dm755 "$repo/target/release/omarchy-mihomod" "$bin_dir/omarchy-mihomod"
omm="$bin_dir/omm"

say "installing services"
install -Dm644 "$repo/packaging/systemd/omarchy-mihomod.service" "$unit_dir/omarchy-mihomod.service"
install -Dm644 "$repo/packaging/systemd/omarchy-mihomo-core.service" "$unit_dir/omarchy-mihomo-core.service"
systemctl --user daemon-reload

if [[ ! -x $core_bin || $upgrade_core == 1 ]]; then
  say "installing the mihomo core"
  if ! "$omm" core install; then
    # GitHub may be unreachable directly; retry through a running local proxy.
    warn "direct download failed, retrying through 127.0.0.1:7897"
    "$omm" core install --proxy http://127.0.0.1:7897
  fi
fi

if ! getcap "$core_bin" 2>/dev/null | grep -q cap_net_admin; then
  if [[ $setcap == yes ]] || ask "TUN needs capabilities on $core_bin (sudo setcap). Grant now?"; then
    "$omm" core setcap
  else
    warn "TUN will not work until you run: omm core setcap"
  fi
fi

say "starting the daemon"
systemctl --user enable omarchy-mihomod.service
if ! systemctl --user restart omarchy-mihomod.service; then
  warn "daemon failed to start: journalctl --user -u omarchy-mihomod"
fi
cvr_running=0
if systemctl is-active --quiet clash-verge-service.service 2>/dev/null || pgrep -x verge-mihomo >/dev/null; then
  cvr_running=1
fi
# Autostarting our core next to Clash Verge would put two TUNs on one machine.
if [[ $cvr_running == 0 ]]; then
  systemctl --user enable omarchy-mihomo-core.service >/dev/null 2>&1 || true
  if [[ -f $data_dir/runtime/config.yaml ]]; then
    say "starting the core"
    systemctl --user restart omarchy-mihomo-core.service
  fi
fi

if [[ $plugin == 1 ]] && command -v omarchy >/dev/null; then
  say "installing the shell plugin"
  mkdir -p "$plugin_dir/plugin"
  install -m644 "$repo/manifest.json" "$plugin_dir/manifest.json"
  rsync -a --delete "$repo/plugin/" "$plugin_dir/plugin/"
  omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  if ! grep -q "\"$plugin_id\"" "$HOME/.config/omarchy/shell.json" 2>/dev/null; then
    omarchy plugin enable "$plugin_id" --section right >/dev/null 2>&1 || warn "enable the widget with: omarchy plugin enable $plugin_id"
  elif grep -q '"socketPath"' "$HOME/.config/omarchy/shell.json" 2>/dev/null; then
    # A development socket override would point the widget at the wrong daemon.
    omarchy bar set "$plugin_id" socketPath "" >/dev/null 2>&1 || true
  fi
fi

echo
"$omm" doctor || true
if [[ $cvr_running == 1 ]]; then
  echo
  say "Clash Verge Rev is still running. Next step: omm migrate cvr"
fi
