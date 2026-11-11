"""SiteCard component set and the «Проверка» item in the Rail.

Run once; then `ids.py`. Variants: State=Idle|Testing|Good|Slow|Fail, Focus=No|Yes (Focus only for Good).
"""
from lib import *

G.update(sites=chr(0xf04c5), dots=chr(0xf01d9))

# State → (dot, latency colour, latency text, route text, border, fill)
STATES = {
    "Idle": (FAINT, FAINT, "—", "не проверен", LINE, CARD),
    "Testing": (ACCENT, DIM, "проверка…", "Европа · NL-Amsterdam-02", ACCENT + "80", CARD),
    "Good": (GREEN, GREEN, "48 мс", "Европа · NL-Amsterdam-02", LINE, CARD),
    "Slow": (YELLOW, YELLOW, "740 мс", "Европа · NL-Amsterdam-02", LINE, CARD),
    "Fail": (RED, RED, "нет ответа", "Европа · NL-Amsterdam-02", LINE, CARD),
}
PROPS = [("Icon", "text", "Icon", None), ("Name", "text", "Name", None), ("Host", "text", "Host", None),
         ("Latency", "text", "Latency", None), ("Route", "text", "Route", None)]


def card(b, state, focus):
    dot, lat_c, lat, route, border, fill = STATES[state]
    if focus:
        border, fill = TOKENS["highlight-border"], TOKENS["card-active"]
    c = b.frame(None, f"State={state}, Focus={'Yes' if focus else 'No'}", "vertical", w=274, gap=12, padding=12,
                fill=fill, stroke=border, strokeWeight=1, strokeAlign="inside", cornerRadius=8)
    top = b.row(c, "Top", gap=10, sizingH="fill")
    tile = b.frame(top, "Tile", "horizontal", w=32, h=32, fill=SURFACE, cornerRadius=8, mainAlign="center", crossAlign="center")
    b.text(tile, chr(0xf05c3), 18, RED, name="Icon")
    t = b.col(top, "Texts", gap=2, sizingH="fill")
    b.text(t, "YouTube", 13, TEXT, bold=True, name="Name")
    b.text(t, "youtube.com", 11, FAINT, name="Host")
    if focus:
        btn(b, top, icon=G["dots"], kind="Ghost", small=True)
    st = b.row(c, "Status", gap=6, sizingH="fill")
    b.ellipse(st, "Dot", 6, dot)
    b.text(st, lat, 13, lat_c, bold=True, name="Latency")
    b.text(st, route, 11, FAINT if state == "Idle" else DIM, name="Route", sizingH="fill", textAutoResize="truncate", textAlignHorizontal="right")
    return c


def main():
    fc, b = FC(), B()
    frames = [card(b, s, False) for s in STATES] + [card(b, "Good", True)]
    comps = [b.add("component.create", {"ids": [f]}) for f in frames]
    # The Rail: «Проверка» after «Прокси»; the following items shift by one, «Настройки» becomes 0.
    rail = C("Rail")
    nav = b.inst(rail, "NavItem", {"Icon": G["sites"], "Label": "Проверка", "Index": "5"})
    b.add("node.reorder", {"id": nav, "index": 5})
    res = run(fc, b, "SiteCard")
    vids = [(res[c["$step"]] or {}).get("id") or res[f["$step"]]["id"] for c, f in zip(comps, frames)]
    sid = fc.cmd("component.combineVariants", ids=vids)["id"]
    fc.tool("set_properties", {"ids": [sid], "props": {"name": "SiteCard", "x": 9300, "y": 3900}})
    trees = [fc.node(i, 6) for i in vids]
    for pname, ptype, layer, _ in PROPS:
        fc.cmd("component.addProperty", id=sid, name=pname, type=ptype, layer=find(trees[0], layer)["id"])
    fc.batch([{"command": "component.bindProperty", "params": {"layer": find(t, layer)["id"], "property": pname, "field": "characters"}}
              for t in trees[1:] for pname, _, layer, _ in PROPS], "SiteCard · properties")
    # Renumber the Rail.
    sets = []
    for c in fc.node(rail, 1)["children"]:
        p = c.get("componentProperties") or {}
        n = {"Профили": "6", "Соединения": "7", "Трафик": "8", "Логи": "9", "Настройки": "0"}.get(p.get("Label"))
        if n:
            sets.append({"command": "instance.setProperties", "params": {"ids": [c["id"]], "properties": {"Index": n}}})
    fc.batch(sets, "Rail · Проверка")
    print("SiteCard", sid, vids)


if __name__ == "__main__":
    main()
