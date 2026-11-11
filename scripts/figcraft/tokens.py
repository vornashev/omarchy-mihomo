#!/usr/bin/env python3
"""Sync colour variables and text styles with lib.py, and bind the document to them.

  tokens.py            create or update the variables and text styles
  tokens.py --bind     also bind every matching solid colour on the page to its variable
  tokens.py --styles   also apply text styles to texts by size and weight
"""
import sys

from lib import *


def hexof(col):
    return "#%02X%02X%02X" % tuple(round(col[k] * 255) for k in "rgb")


def sync_variables(fc):
    have = fc.cmd("variable.list")
    cols = {c["name"]: c for c in have.get("collections", [])}
    if COLLECTION not in cols:
        fc.cmd("variable.createCollection", name=COLLECTION, modes=[MODE])
        have = fc.cmd("variable.list")
    names = {v["name"]: v for v in have.get("variables", [])}
    cmds = []
    for name, value in [("color/" + k, v) for k, v in TOKENS.items()] + [("canvas/" + k, v) for k, v in CANVAS.items()]:
        if name in names:
            cmds.append({"command": "variable.set", "params": {"variable": names[name]["id"], "value": value}})
        else:
            cmds.append({"command": "variable.create", "params": {"collection": COLLECTION, "name": name, "type": "color", "value": value}})
    fc.batch(cmds, "Sync colour variables")
    print(len(cmds), "colour variables")


def alpha(p):
    """A solid paint's total opacity: paint opacity × colour alpha."""
    c = str(p.get("color", ""))
    return p.get("opacity", 1) * (int(c[7:9], 16) / 255 if len(c) == 9 else 1)


def scan(fc, ids):
    """(ids of instances and everything inside them, {id: paints} of layers with translucent token paints)."""
    inside, tinted = set(), {}

    def walk(n, inst):
        inst = inst or n.get("type") == "INSTANCE"
        if inst:
            inside.add(n["id"])
        for key in ("fills", "strokes"):
            ps = n.get(key) or []
            if any(p.get("type") == "solid" and alpha(p) < 0.999 and str(p.get("color", ""))[:7].upper() in VAR_FOR_HEX for p in ps):
                tinted.setdefault(n["id"], {})[key] = ps
        for c in n.get("children") or []:
            walk(c, inst)

    for i in ids:
        walk(fc.node(i, 40), False)
    return inside, tinted


def retint(paints):
    """The same paints with the tint as paint opacity: binding a variable resets the colour, and in a large batch the opacity too."""
    out = []
    for p in paints:
        p = dict(p)
        if p.get("type") == "solid" and str(p.get("color", ""))[:7].upper() in VAR_FOR_HEX:
            p.update(opacity=round(alpha(p), 4), color=str(p["color"])[:7])
        out.append(p)
    return out


def bind_colors(fc, ids=None, overrides=False):
    """Bind own layers first (instances then inherit from their mains); with overrides, also what instances override."""
    ids = ids or [c["id"] for c in fc.tops()]
    inside, tinted = scan(fc, ids)
    skip = set() if overrides else inside
    cmds, bound, skipped = [], set(), {}
    for c in fc.cmd("selection.colors", ids=ids)["colors"]:
        col = c["paint"].get("color")
        if c.get("variable") or not isinstance(col, dict):
            continue
        targets = [t for t in c["targets"] if t["id"] not in skip]
        var = VAR_FOR_HEX.get(hexof(col))
        if var and targets:
            cmds.append({"command": "selection.bindColorVariable", "params": {"targets": targets, "variable": var}})
            bound.update(t["id"] for t in targets)
        elif not var:
            skipped[hexof(col)] = skipped.get(hexof(col), 0) + len(c["targets"])
    tints = [i for i in tinted if i in bound]
    cmds += [{"command": "node.set", "params": {"ids": [i], "props": {k: retint(v) for k, v in tinted[i].items()}}} for i in tints]
    if cmds:
        fc.batch(cmds, "Bind colour overrides" if overrides else "Bind colours to variables")
    print(len(bound), "layers bound" + (" (overrides)" if overrides else ""), f"({len(tints)} tinted);", "not tokens:", skipped)


def sync_styles(fc):
    have = {s["name"]: s for s in fc.cmd("style.list", type="text")}
    missing = [(k, v) for k, v in TEXT_STYLES.items() if v not in have]
    if not missing:
        return have
    b = B(bind=False)
    tmp = b.frame(None, "_text-style-samples", "vertical", x=-20000, y=-20000)
    refs = [(name, b.text(tmp, name, size, TEXT, bold=bold)) for (size, bold), name in missing]
    res = fc.batch(b.cmds, "Text style samples")
    cmds = [{"command": "style.create", "params": {"type": "text", "name": name, "from": res[r["$step"]]["id"]}} for name, r in refs]
    cmds.append({"command": "edit.delete", "params": {"ids": [res[tmp["$step"]]["id"]]}})
    fc.batch(cmds, "Create text styles")
    print(len(missing), "text styles created")
    return {s["name"]: s for s in fc.cmd("style.list", type="text")}


def apply_styles(fc):
    tops = [c["id"] for c in fc.tops()]
    groups = {}

    def walk(n):
        if n.get("type") == "TEXT" and not n.get("textStyle") and n.get("fontFamily") == FONT:
            name = TEXT_STYLES.get((round(n.get("fontSize", 0)), n.get("fontStyle") == "Bold"))
            if name and n.get("letterSpacing") in (None, "0%", 0) and n.get("lineHeight") in (None, "Auto"):
                groups.setdefault(name, []).append(n["id"])
        if n.get("type") != "INSTANCE":  # instances follow their main component
            for c in n.get("children") or []:
                walk(c)

    for t in tops:
        walk(fc.node(t, 30))
    cmds = [{"command": "style.apply", "params": {"ids": ids, "style": name}} for name, ids in groups.items()]
    if cmds:
        fc.batch(cmds, "Apply text styles")
    print({k: len(v) for k, v in groups.items()})


if __name__ == "__main__":
    fc = FC()
    sync_variables(fc)
    sync_styles(fc)
    if "--bind" in sys.argv:
        bind_colors(fc)
        bind_colors(fc, overrides=True)
    if "--styles" in sys.argv:
        apply_styles(fc)
