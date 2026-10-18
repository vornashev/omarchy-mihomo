#!/usr/bin/env python3
"""Regenerate the offline form catalogue from pinned upstream option structs.

Network is used only by this maintainer script, never by the editor.
"""
import hashlib
import io
import json
import pathlib
import re
import tarfile
import urllib.request

VERSION = "v1.19.32"
ROOT = pathlib.Path(__file__).resolve().parents[1]
archive = urllib.request.urlopen(f"https://codeload.github.com/MetaCubeX/mihomo/tar.gz/refs/tags/{VERSION}", timeout=60).read()
with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
    all_sources = {m.name.split("/", 1)[1]: tar.extractfile(m).read().decode() for m in tar.getmembers() if m.isfile() and m.name.endswith(".go")}

def fetch(path):
    return all_sources[path]

paths = [p for p in all_sources if re.match(r"adapter/(outbound|outboundgroup)/[^/]+\.go$", p) and not p.endswith("_test.go")]
sources = {p:fetch(p) for p in paths}
structs = {}
for path, source in sources.items():
    for match in re.finditer(r"type (\w+) struct \{(.*?)\n\}", source, re.S):
        structs[match[1]] = (match[2], path)

def shape(typ, seen=()):
    typ = typ.lstrip("*")
    if typ.startswith("[]"):
        return {"kind": "list", "item": shape(typ[2:], seen)}
    if typ.startswith("map["):
        return {"kind": "map", "item": shape(typ.split("]", 1)[1], seen)}
    if typ in structs and typ not in seen:
        return {"kind": "object", "fields": fields(typ, seen + (typ,))}
    if re.fullmatch(r"u?int\d*|float\d+|byte", typ):
        return {"kind": "number"}
    if typ == "bool":
        return {"kind": "boolean"}
    if typ in ("any", "interface{}"):
        return {"kind": "json"}
    return {"kind": "string"}

CHOICES = {
    "ip-version": ["dual", "ipv4", "ipv6", "ipv4-prefer", "ipv6-prefer"],
    "strategy": ["consistent-hashing", "round-robin", "sticky-sessions"],
    "hash-key": ["", "in-user"],
    "flow": ["", "xtls-rprx-vision"],
}

def fields(name, seen=()):
    body, _ = structs[name]
    result = []
    for line in body.splitlines():
        match = re.match(r'\s*\w+\s+(\S+)\s+`(?:proxy|group):"([^"]+)', line)
        if match and match[2] != "-":
            key = match[2].split(",")[0]
            f = {"key": key, "required": "omitempty" not in match[2], **shape(match[1], seen)}
            if key in CHOICES:
                f["choices"] = CHOICES[key]
            if any(s in key for s in ("password", "private-key", "pre-shared-key", "token", "secret")) or key in ("key", "auth-key", "uuid", "psk", "auth", "auth-str"):
                f["secret"] = True
            if key in ("i1", "i2", "i3", "i4", "i5", "certificate", "ca", "cert", "key", "private-key", "tls-auth", "tls-crypt", "tls-crypt-v2"):
                f["multiline"] = True
            transport = {"ws-opts":"ws", "ws-headers":"ws", "grpc-opts":"grpc", "h2-opts":"h2", "http-opts":"http", "xhttp-opts":"xhttp", "mkcp-opts":"kcp", "mekya-opts":"mekya"}
            if key in transport:
                f["when"] = {"network": transport[key]}
            result.append(f)
        elif re.fullmatch(r"\s*\w+\s*", line) and line.strip() in structs:
            result.extend(fields(line.strip(), seen + (name,)))
    return list({f["key"]: f for f in result}.values())

parser = fetch("adapter/parser.go")
nodes = []
for kind, option in re.findall(r'case "([^\"]+)":\s*\w+ := &outbound\.(\w+)\{', parser):
    fs = fields(option)
    if kind not in ("direct", "dns", "reject", "rematch"):
        fs.append({"key": "smux", **shape("SingMuxOption")})
    priority = ["name", "server", "port", "uuid", "username", "password", "cipher", "private-key", "public-key", "ip", "ipv6", "network", "tls", "servername", "sni", "reality-opts", "amnezia-wg-option", "peers"]
    fs.sort(key=lambda f: priority.index(f["key"]) if f["key"] in priority else len(priority))
    for f in fs:
        if f["key"] == "network":
            f["choices"] = {"vless":["tcp","ws","grpc","h2","http","xhttp"], "vmess":["tcp","ws","grpc","h2","http","kcp","mekya"], "trojan":["tcp","ws","grpc"]}.get(kind, [])
    nodes.append({"type": kind, "fields": fs, "source": structs[option][1]})
groups = []
for kind, option in [("select", "SelectorOption"), ("url-test", "URLTestOption"), ("fallback", "FallbackOption"), ("load-balance", "LoadBalanceOption")]:
    fs = [f for f in fields("GroupCommonOption") if f["key"] != "type"]
    if option in structs:
        fs += fields(option)
    groups.append({"type": kind, "fields": fs})
out = {"version": VERSION, "archive_sha256": hashlib.sha256(archive).hexdigest(), "nodes": nodes, "groups": groups}
path = ROOT / "crates/omm-config/src/proxy_catalog.json"
path.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n")
print(f"{len(nodes)} proxy types, {len(groups)} group types → {path}")
