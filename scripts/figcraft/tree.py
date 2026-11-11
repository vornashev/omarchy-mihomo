#!/usr/bin/env python3
"""Print a layer tree one line per layer, instead of get_node's full JSON.

  tree.py                 top-level layers of the current page
  tree.py ID [DEPTH]      a subtree (default depth 3)
  tree.py --find NAME     ids of layers named NAME (whole page)
"""
import sys

from fc import FC


def paints(n, key):
    out = []
    for p in n.get(key) or []:
        c = p.get("color")
        out.append(c if isinstance(c, str) else p.get("type", "?"))
    return ",".join(out)


def line(n, depth):
    s = f"{'  ' * depth}{n.get('id')} {n.get('type')} '{n.get('name')}' {n.get('width', 0):.0f}x{n.get('height', 0):.0f}"
    if depth == 0 or n.get("type") in ("FRAME", "COMPONENT", "SECTION"):
        s += f" @{n.get('x', 0):.0f},{n.get('y', 0):.0f}"
    if paints(n, "fills"):
        s += " fill=" + paints(n, "fills")
    if paints(n, "strokes"):
        s += " stroke=" + paints(n, "strokes")
    if n.get("boundVariables"):
        s += " vars=" + ",".join(sorted(n["boundVariables"]))
    lay = n.get("layout") or {}
    if lay.get("mode") not in (None, "none"):
        s += f" {lay['mode'][0].upper()} gap={lay['gap']:g}"
    if n.get("characters") is not None:
        s += f" {n.get('fontSize', 0):g}{'b' if n.get('fontStyle') == 'Bold' else ''} {n['characters'][:60]!r}"
    if n.get("componentProperties"):
        s += " props=" + ",".join(f"{k}={v}" for k, v in n["componentProperties"].items() if not isinstance(v, str) or len(v) < 30)
    if n.get("visible") is False:
        s += " hidden"
    return s


def walk(n, depth, out):
    out.append(line(n, depth))
    for c in n.get("children") or []:
        walk(c, depth + 1, out)


def collect(n, name, out):
    if n.get("name") == name:
        out.append(f"{n['id']} {n['type']} {n.get('width', 0):.0f}x{n.get('height', 0):.0f}")
    for c in n.get("children") or []:
        collect(c, name, out)


if __name__ == "__main__":
    fc = FC()
    args = sys.argv[1:]
    out = []
    if not args:
        for c in fc.tops():
            out.append(line(c, 0))
    elif args[0] == "--find":
        for c in fc.tops():
            collect(fc.node(c["id"], 12), args[1], out)
    else:
        walk(fc.node(int(args[0]), int(args[1]) if len(args) > 1 else 3), 0, out)
    print("\n".join(out))
