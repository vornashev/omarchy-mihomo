"""Step 1 of the presentation: English copies of the screens on the page «Презентация».

Copies the source frames there, sets Russian component properties and texts to
English (en_strings.py) and writes {source id: copy id} to present_ids.json.
Prints only what it could not translate.
"""
import json
import sys

from fc import FC
from en_strings import CYR, en

SOURCES = {"scene": 6041, "problems": 1772, "rules": 2308, "traffic": 832, "notice": 5124}
OUT = __import__("pathlib").Path(__file__).with_name("present_ids.json")


def walk(n, fn):
    fn(n)
    for c in n.get("children") or []:
        walk(c, fn)


def translate(fc, root_id):
    tree = fc.node(root_id, 30)
    props = []

    def inst(n):
        cp = n.get("componentProperties") or {}
        p = {k: en(v) for k, v in cp.items() if isinstance(v, str) and CYR.search(v)}
        if p:
            props.append({"command": "instance.setProperties", "params": {"ids": [n["id"]], "properties": p}})
    walk(tree, inst)
    if props:
        fc.batch(props, "Presentation · properties to English")
    tree = fc.node(root_id, 30)
    sets, miss = [], []

    def text(n):
        s = n.get("characters") or ""
        if n.get("type") == "TEXT" and CYR.search(s):
            e = en(s)
            if CYR.search(e):
                miss.append(s)
            sets.append({"command": "node.set", "params": {"ids": [n["id"]], "props": {"characters": e}}})
    walk(tree, text)
    if sets:
        fc.batch(sets, "Presentation · texts to English")
    return miss


def main():
    fc = FC()
    pages = fc.cmd("page.list")
    page = next((p for p in pages if p["name"] == "Презентация"), None)
    if page is None:
        page = fc.cmd("page.add", name="Презентация")
        print("page.add →", page)
        fc.cmd("page.select", id=1)
    page_id = page["id"]
    ids = json.loads(OUT.read_text()) if OUT.exists() else {}
    for key, src in SOURCES.items():
        if key in ids and "--again" not in sys.argv:
            continue
        r = fc.cmd("edit.duplicate", ids=[src], dx=0, dy=20000)
        copy = (r.get("ids") or [r.get("id")])[0] if isinstance(r, dict) else r[0]
        if not isinstance(copy, (int, str)):
            raise SystemExit(f"unexpected duplicate result: {str(r)[:300]}")
        fc.cmd("object.moveToPage", ids=[copy], page=page_id)
        ids[key] = copy
        OUT.write_text(json.dumps(ids, indent=1))
        miss = translate(fc, copy)
        print(key, copy, "untranslated:", miss or "none")
    ids["page"] = page_id
    OUT.write_text(json.dumps(ids, indent=1))


if __name__ == "__main__":
    main()
