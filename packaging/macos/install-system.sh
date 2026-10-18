#!/bin/bash
# Called only by install.sh under sudo. No network downloads run as root.
set -euo pipefail
[[ $EUID == 0 && $# == 2 ]] || exit 1
app=$1
user_id=$2
[[ $user_id =~ ^[0-9]+$ && $user_id != 0 ]] || exit 1
python3 - "$user_id" <<'PY'
import pwd,sys
pwd.getpwuid(int(sys.argv[1]))
PY
codesign --verify --deep --strict "$app"
label=io.github.vornashev.omarchy-mihomo.helper
root=/Library/PrivilegedHelperTools/omarchy-mihomo
if [[ -e "$root/owner" && $(cat "$root/owner") != "$user_id" ]]; then echo 'The network helper is installed for another user. Uninstall it first.' >&2; exit 1; fi
launchctl bootout "system/$label" 2>/dev/null || true
# Retain one previous package for manual rollback; never delete user profiles.
if [[ -d /Applications/Mihomo.app ]]; then
  [[ ! -e /Applications/Mihomo.previous.app ]] || /bin/rm -rf /Applications/Mihomo.previous.app
  mv /Applications/Mihomo.app /Applications/Mihomo.previous.app
fi
ditto "$app" /Applications/Mihomo.app
chown -R root:wheel /Applications/Mihomo.app
mkdir -p "$root" "/Library/Application Support/omarchy-mihomo/$user_id/runtime"
chmod 755 "$root"
chmod 700 "/Library/Application Support/omarchy-mihomo" "/Library/Application Support/omarchy-mihomo/$user_id" "/Library/Application Support/omarchy-mihomo/$user_id/runtime"
for bin in omm omarchy-mihomod omm-network-helper omm-system-proxy mihomo; do
  install -m 755 -o root -g wheel "/Applications/Mihomo.app/Contents/Resources/bin/$bin" "$root/$bin"
done
printf '%s\n' "$user_id" > "$root/owner"
chmod 644 "$root/owner"
python3 - "$user_id" <<'PY'
import plistlib,pathlib,sys
uid=int(sys.argv[1]);label='io.github.vornashev.omarchy-mihomo.helper'
p=pathlib.Path('/Library/LaunchDaemons')/(label+'.plist')
root=f'/Library/Application Support/omarchy-mihomo/{uid}'
data={'Label':label,'ProgramArguments':['/Library/PrivilegedHelperTools/omarchy-mihomo/omm-network-helper','--uid',str(uid)],'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'ExitTimeOut':20,'StandardOutPath':root+'/helper.log','StandardErrorPath':root+'/helper.log'}
p.write_bytes(plistlib.dumps(data));p.chmod(0o644)
PY
launchctl bootstrap system "/Library/LaunchDaemons/$label.plist"
