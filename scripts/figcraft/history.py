#!/usr/bin/env python3
"""Show the last undo steps; with --wait, block until the document is quiet.

Another agent may be editing the same document: the undo history is shared.
  history.py            last steps and the dirty flag
  history.py --wait N   return once nothing changed for N seconds (default 180)
"""
import sys
import time

from fc import FC

fc = FC()
h = fc.doc(0)
print("dirty" if h["dirty"] else "saved", "·", " | ".join(h["history"]["undo"][:6]))
if "--wait" in sys.argv:
    quiet = int(sys.argv[-1]) if sys.argv[-1].isdigit() else 180
    last, since = None, time.time()
    while time.time() - since < quiet:
        key = tuple(fc.doc(0)["history"]["undo"][:5])
        if key != last:
            last, since = key, time.time()
        time.sleep(15)
    print("quiet for", quiet, "s")
