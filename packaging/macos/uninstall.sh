#!/bin/bash
set -euo pipefail
[[ $(uname -s) == Darwin && $EUID != 0 ]] || { echo 'Run as the installed macOS user.' >&2; exit 1; }
user_id=$(id -u)
root=/Library/PrivilegedHelperTools/omarchy-mihomo
[[ ! -f "$root/owner" || $(cat "$root/owner") == "$user_id" ]] || { echo 'Installed for another user.' >&2; exit 1; }
"$root/omm" core stop 2>/dev/null || true
launchctl bootout "gui/$user_id/io.github.vornashev.omarchy-mihomo.daemon" 2>/dev/null || true
sudo launchctl bootout system/io.github.vornashev.omarchy-mihomo.helper 2>/dev/null || true
# Retry journal recovery even if the helper was not running. Fail instead of
# deleting the recovery journal if restoration cannot complete.
if [[ -x "$root/omm-system-proxy" ]]; then sudo "$root/omm-system-proxy" restore 0 "/Library/Application Support/omarchy-mihomo/$user_id/proxy-state.json"; fi
sudo rm -f /Library/LaunchDaemons/io.github.vornashev.omarchy-mihomo.helper.plist
sudo rm -rf "$root" /Applications/Mihomo.app /Applications/Mihomo.previous.app
rm -f "$HOME/Library/LaunchAgents/io.github.vornashev.omarchy-mihomo.daemon.plist" "$HOME/.omm/daemon.sock"
[[ ! -L "$HOME/.local/bin/omm" ]] || rm "$HOME/.local/bin/omm"
echo 'Removed the application and services. Profiles, settings and recovery logs were preserved.'
