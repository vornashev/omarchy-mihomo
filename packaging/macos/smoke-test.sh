#!/bin/bash
# Ephemeral CI Mac only: no system proxy or TUN changes.
set -euo pipefail
[[ ${CI:-} == true && $(uname -s) == Darwin ]] || { echo 'Use only on an ephemeral macOS CI runner.' >&2; exit 1; }
cd "$(dirname "$0")/../.."
cleanup() { packaging/macos/uninstall.sh || true; }
trap cleanup EXIT
packaging/macos/install.sh
python3 - <<'PY'
import json,os,pathlib,socket,time,urllib.request,http.server,threading,subprocess
sock=str(pathlib.Path.home()/'.omm/daemon.sock')
class RPC:
 def __init__(self):
  self.s=socket.socket(socket.AF_UNIX);self.s.settimeout(80);self.s.connect(sock);self.f=self.s.makefile('rwb');self.id=0
 def call(self,method,params={}):
  self.id+=1;self.f.write((json.dumps({'id':self.id,'method':method,'params':params})+'\n').encode());self.f.flush()
  while True:
   r=json.loads(self.f.readline())
   if r.get('id')!=self.id:continue
   if 'error' in r:raise RuntimeError(r['error'])
   return r['result']
 def close(self):self.f.close();self.s.close()
for _ in range(50):
 try:rpc=RPC();break
 except (OSError,ValueError):time.sleep(.2)
else:raise RuntimeError('daemon did not start')
for _ in range(50):
 if rpc.call('status')['helper']['available']:break
 time.sleep(.2)
else:raise RuntimeError('helper unavailable')
status=rpc.call('status');assert not status['core']['running']
profile='mixed-port: 7897\ntun: {enable: false}\nproxies: []\nproxy-groups: [{name: Test, type: select, proxies: [DIRECT]}]\nrules: ["MATCH,Test"]\n'
rpc.call('profiles.add',{'name':'Smoke','content':profile})
rpc.call('session.open')
rpc.call('session.start')
for _ in range(50):
 status=rpc.call('status')
 if status['core']['running']:break
 time.sleep(.2)
assert status['core']['running'] and not status['core']['tun'],status
assert not status['system_proxy']['enabled'],status
assert rpc.call('proxies.list')
rpc.call('mode.set',{'mode':'direct'})
rpc.call('mode.set',{'mode':'rule'})
rpc.call('config.apply')
# Root controller must not expose arbitrary reloads through its public relay.
api=f'/var/run/omarchy-mihomo/{os.getuid()}/api.sock'
s=socket.socket(socket.AF_UNIX);s.connect(api);s.sendall(b'PUT /configs HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n');response=s.recv(4096);s.close();assert b'400' in response.split(b'\r\n',1)[0],response
# Closing the owning UI connection must release the session.
rpc.close();rpc=RPC()
for _ in range(80):
 if not rpc.call('status')['core']['running']:break
 time.sleep(.2)
else:raise RuntimeError('session survived owner disconnection')
rpc.call('session.start');rpc.call('session.stop');rpc.call('session.stop')
rpc.close()
# Verify the packaged app can find its localized resources and connect.
subprocess.run(['open','/Applications/Mihomo.app'],check=True)
time.sleep(3)
subprocess.run(['pgrep','-x','Mihomo'],check=True)
subprocess.run(['osascript','-e','tell application id "io.github.vornashev.omarchy-mihomo" to quit'],check=True,timeout=20)
for _ in range(60):
 if subprocess.run(['pgrep','-x','Mihomo'],stdout=subprocess.DEVNULL).returncode!=0:break
 time.sleep(.2)
else:raise RuntimeError('application did not quit')
print('PASS: installation, profile, core start/reload/stop, filtered API, lease cleanup, packaged app launch/quit')
PY
