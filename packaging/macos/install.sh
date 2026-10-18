#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { echo 'This installer requires an Apple Silicon Mac.' >&2; exit 1; }
[[ $EUID != 0 ]] || { echo 'Run this script as your normal user; it invokes sudo only for system installation.' >&2; exit 1; }
app="$PWD/target/macos/Mihomo.app"
[[ -x "$app/Contents/MacOS/Mihomo" ]] || { echo 'Run packaging/macos/build.sh first.' >&2; exit 1; }
user_id=$(id -u)
log_dir="$HOME/Library/Logs/omarchy-mihomo"
mkdir -p "$log_dir" "$HOME/Library/LaunchAgents" "$HOME/.omm" "$HOME/.local/bin"
chmod 700 "$log_dir" "$HOME/.omm"
# Stop the user daemon first so the old helper can restore the network.
launchctl bootout "gui/$user_id/io.github.vornashev.omarchy-mihomo.daemon" 2>/dev/null || true
sudo /bin/bash packaging/macos/install-system.sh "$app" "$user_id"
python3 - "$HOME" "$user_id" <<'PY'
import plistlib,sys,pathlib,os
home=pathlib.Path(sys.argv[1]); uid=int(sys.argv[2]); label='io.github.vornashev.omarchy-mihomo.daemon'
p=home/'Library/LaunchAgents'/f'{label}.plist'
data={'Label':label,'ProgramArguments':['/Library/PrivilegedHelperTools/omarchy-mihomo/omarchy-mihomod'], 'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{'HOME':str(home),'OMM_SOCKET':str(home/'.omm/daemon.sock'),'OMM_LOG':'info'},'StandardOutPath':str(home/'Library/Logs/omarchy-mihomo/daemon.log'),'StandardErrorPath':str(home/'Library/Logs/omarchy-mihomo/daemon.log')}
p.write_bytes(plistlib.dumps(data));p.chmod(0o600)
PY
launchctl bootstrap "gui/$user_id" "$HOME/Library/LaunchAgents/io.github.vornashev.omarchy-mihomo.daemon.plist"
ln -sfn /Library/PrivilegedHelperTools/omarchy-mihomo/omm "$HOME/.local/bin/omm"
echo 'Installed. Open /Applications/Mihomo.app and import a profile.'
