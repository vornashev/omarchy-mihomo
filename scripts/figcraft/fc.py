"""Minimal MCP client for a running FigCraft app (`figcraft --control PORT`).

Scripts talk to the app through `figcraft-cli mcp --attach`, so command
results stay in the script instead of an agent's context.
"""
import base64
import glob
import json
import os
import shutil
import subprocess
import sys
import time


def _cli():
    if os.environ.get("FIGCRAFT_CLI"):
        return [os.environ["FIGCRAFT_CLI"]]
    if shutil.which("figcraft-cli"):
        return ["figcraft-cli"]
    images = sorted(glob.glob(os.path.expanduser("~/Applications/figcraft-*.AppImage")))
    if images:
        return [images[-1], "cli"]
    if shutil.which("figcraft"):
        return ["figcraft", "cli"]
    raise SystemExit("figcraft-cli not found: set FIGCRAFT_CLI")


class FC:
    def __init__(self, port=None):
        port = str(port or os.environ.get("FIGCRAFT_PORT", "7979"))
        self.p = subprocess.Popen(_cli() + ["mcp", "--attach", port], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, text=True, bufsize=1)
        self.n = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "scripts/figcraft", "version": "1"}})
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, msg):
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()

    def rpc(self, method, params):
        self.n += 1
        self._send({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params})
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("figcraft-cli closed the connection (is the app running with --control?)")
            m = json.loads(line)
            if m.get("id") == self.n:
                if "error" in m:
                    raise RuntimeError(m["error"])
                return m["result"]

    def call(self, name, args, retries=12):
        """Raw tools/call result; retries while the app window is hidden."""
        r = self.rpc("tools/call", {"name": name, "arguments": args})
        if r.get("isError"):
            txt = "".join(c.get("text", "") for c in r.get("content", []) if c.get("type") == "text")
            # The app answers only while its window is on screen (another workspace pauses it).
            if "waiting for the UI" in txt and retries:
                if retries == 12:
                    print("FigCraft window is hidden; waiting for it…", file=sys.stderr)
                time.sleep(5)
                return self.call(name, args, retries - 1)
            raise RuntimeError(txt[:2000])
        return r

    def tool(self, name, args, retries=12):
        r = self.call(name, args, retries)
        txt = "".join(c.get("text", "") for c in r.get("content", []) if c.get("type") == "text")
        try:
            return json.loads(txt)
        except ValueError:
            return txt

    def shot(self, id, path, max_size=1000):
        """Save a PNG of one layer to `path` (look at it with Read only when needed)."""
        r = self.call("screenshot", {"ids": [id], "maxSize": max_size})
        img = next(c for c in r["content"] if c.get("type") == "image")
        with open(path, "wb") as f:
            f.write(base64.b64decode(img["data"]))
        return path

    def cmd(self, command, **params):
        return self.tool("run_command", {"command": command, "params": params})

    def batch(self, commands, label="Batch"):
        return self.tool("batch", {"commands": commands, "label": label})

    def node(self, id, depth=1):
        return self.tool("get_node", {"id": id, "depth": depth})

    def doc(self, depth=0):
        return self.tool("inspect_document", {"depth": depth})

    def tops(self, doc=None):
        """Top-level layers of the current page (inspect_document lists only that page's tree)."""
        doc = doc or self.doc(1)
        page = next(p for p in doc["pages"] if p["id"] == doc.get("currentPage", doc["pages"][0]["id"]))
        return page.get("children") or []
