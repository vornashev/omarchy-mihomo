#!/usr/bin/env python3
"""Comment threads as remarks for an agent: what is said and which layer it is pinned to.

  comments.py                 open threads on every page
  comments.py --all           resolved ones too
  comments.py reply ID TEXT   answer in a thread (as "Claude")
  comments.py resolve ID      close a thread
"""
import sys

from fc import FC
from selection import describe

AUTHOR = "Claude"


def under_pin(fc, t):
    """Innermost layer at the pin (the anchor is often just the frame around it)."""
    if t.get("page") not in (None, fc.doc(0).get("currentPage")):
        return None
    stack = fc.cmd("select.layersAt", x=t.get("x", 0), y=t.get("y", 0)).get("layers") or []
    deepest = None
    for layer in stack:  # the first branch, top-down, ends where the depth stops growing
        if deepest and layer["depth"] <= deepest["depth"]:
            break
        deepest = layer
    return deepest and deepest["id"]

if __name__ == "__main__":
    fc = FC()
    args = sys.argv[1:]
    if args[:1] == ["reply"]:
        fc.cmd("comment.reply", id=int(args[1]), text=" ".join(args[2:]), author=AUTHOR)
        sys.exit()
    if args[:1] == ["resolve"]:
        fc.cmd("comment.resolve", id=int(args[1]))
        sys.exit()
    threads = fc.cmd("comment.list", page="all", **({} if "--all" in args else {"resolved": False}))["comments"]
    tops = fc.tops()
    out = []
    for t in threads:
        out.append(f"comment {t['id']}{' [resolved]' if t.get('resolved') else ''} · {t.get('author')}: {t.get('text')}")
        for r in t.get("replies") or []:
            out.append(f"  ↳ {r.get('author')}: {r.get('text')}")
        out.append(f"  pinned at {t.get('x', 0):.0f},{t.get('y', 0):.0f} to #{t.get('anchor')} in «{t.get('frameName') or t.get('pageName')}»")
        under = under_pin(fc, t)
        if under:
            out.append("  under the pin: " + describe(fc, under, tops))
    print("\n".join(out) or "no open comments")
