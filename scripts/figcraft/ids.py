#!/usr/bin/env python3
"""Refresh ids.json: names of top-level components and component sets → ids.

Run after adding, renaming or deleting components, so scripts and agents
don't have to inspect the document to find them.
"""
import json

from fc import FC
from lib import IDS_FILE

fc = FC()
comps = {}
for c in fc.cmd("component.list"):
    # Variants are reached through their set; skip "Prop=Value" names.
    if "=" in c["name"] or c.get("library"):
        continue
    if c["name"] in comps:
        print("duplicate name, keeping the first:", c["name"])
        continue
    comps[c["name"]] = c["id"]
IDS_FILE.write_text(json.dumps({"components": dict(sorted(comps.items()))}, ensure_ascii=False, indent=1) + "\n")
print(len(comps), "components →", IDS_FILE)
