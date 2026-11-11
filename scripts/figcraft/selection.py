#!/usr/bin/env python3
"""Print the selected layers with their path, so a remark can point at a layer.

  selection.py            the selection, one line per layer
  selection.py ID...      the same for given layers
  selection.py --copy     also put the lines on the clipboard (wl-copy) and notify; bind it to a key

A line reads `figcraft #ID TYPE «name» W×H — [Page / ]Top › ◇Instance › … › name`; ◇ marks an instance,
so a change to a layer below it belongs in a main component listed after `main:` (outermost first).
"""
import subprocess
import sys

from fc import FC

CONTAINERS = ("FRAME", "GROUP", "SECTION", "COMPONENT", "COMPONENT_SET", "INSTANCE", "BOOLEAN_OPERATION")


def _box(n):
    b = n.get("absoluteBounds")
    return (b["x"], b["y"], b["width"], b["height"]) if b else (n.get("x", 0), n.get("y", 0), n.get("width", 0), n.get("height", 0))


def _inside(inner, outer, eps=0.5):
    x, y, w, h = inner
    X, Y, W, H = outer
    return x >= X - eps and y >= Y - eps and x + w <= X + W + eps and y + h <= Y + H + eps


def _find(fc, nid, target, box, chain):
    """Depth-first, only into layers whose bounds contain the target's (cheap get_node calls)."""
    for c in fc.node(nid, 1).get("children") or []:
        if c["id"] == target:
            return chain + [c]
        if c.get("type") in CONTAINERS and c.get("childCount", 1) and _inside(box, _box(c)):
            r = _find(fc, c["id"], target, box, chain + [c])
            if r:
                return r


def _deep(n, target, chain):
    for c in n.get("children") or []:
        if c["id"] == target:
            return chain + [c]
        r = _deep(c, target, chain + [c])
        if r:
            return r


def path_of(fc, target, tops=None):
    """[top, …, layer] for a layer id on the current page, or None."""
    node = fc.node(target, 0)
    box = _box(node)
    tops = tops if tops is not None else fc.tops()
    for t in tops:
        if t["id"] == target:
            return [node]
    hits = [t for t in tops if _inside(box, _box(t))]
    for t in hits:
        r = _find(fc, t["id"], target, box, [t])
        if r:
            r[-1] = node
            return r
    for t in hits or tops:  # a layer may stick out of an unclipped parent: search the whole subtree
        r = _deep(fc.node(t["id"], 60), target, [t])
        if r:
            r[-1] = node
            return r


def describe(fc, target, tops=None, page=None):
    chain = path_of(fc, target, tops) or [fc.node(target, 0)]
    n = chain[-1]
    names = [("◇" if c.get("type") == "INSTANCE" else "") + str(c.get("name")) for c in chain]
    s = f"figcraft #{target} {n.get('type')} «{n.get('name')}» {n.get('width', 0):.0f}×{n.get('height', 0):.0f} — {page + ' / ' if page else ''}{' › '.join(names)}"
    mains = [fc.node(c["id"], 0).get("mainComponentName") for c in chain if c.get("type") == "INSTANCE"]
    if any(mains):
        s += f"  (main: {' › '.join(m for m in mains if m)})"
    if n.get("characters") is not None:
        s += f"  {n['characters'][:60]!r}"
    return s


if __name__ == "__main__":
    args = sys.argv[1:]
    copy = "--copy" in args
    ids = [int(a) for a in args if a.isdigit()]
    try:
        fc = FC()
        doc = fc.doc(1)
        ids = ids or doc.get("selection") or []
        tops = fc.tops(doc)
        pages = doc.get("pages") or []
        page = next((p["name"] for p in pages if p["id"] == doc.get("currentPage")), None) if len(pages) > 1 else None
        lines = [describe(fc, i, tops, page) for i in ids]
    except Exception as e:  # app not running, window hidden too long…
        lines, err = [], str(e)[:200]
    else:
        err = None if lines else "nothing selected"
    print("\n".join(lines) or err)
    if copy:
        if lines:
            subprocess.run(["wl-copy"], input="\n".join(lines), text=True)
        subprocess.run(["notify-send", "-a", "FigCraft", "-t", "3000",
                        f"Copied {len(lines)} layer path(s)" if lines else "Nothing copied", lines[0] if lines else err])
