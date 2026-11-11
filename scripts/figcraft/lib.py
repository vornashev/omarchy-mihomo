"""Building blocks for design.figcraft: tokens, a batch builder and UI helpers.

A script collects layers in a `B` and sends them with `run()` as one undo step.
Colours given as the token constants below are bound to their variables and
text sizes to their text styles, so palette and type changes stay one edit.
"""
import json
import pathlib

from fc import FC

HERE = pathlib.Path(__file__).resolve().parent
FONT = "JetBrainsMono Nerd Font"

# Tokyo Night palette; each opaque token is a colour variable (collection "Tokens").
TOKENS = {
    "bg": "#1F2335", "rail": "#1A1B26", "card": "#24283B", "line": "#2F334D", "border": "#3B4261",
    "text": "#C0CAF5", "text-dim": "#A9B1D6", "text-faint": "#565F89",
    "accent": "#7AA2F7", "accent-hover": "#8FB3FA", "selected": "#34406A", "nav-selected": "#2E3A5C", "hover": "#292E42",
    "surface": "#31354A", "highlight": "#222B45", "highlight-border": "#4A5F9A", "card-hover": "#2A2F45", "card-active": "#26304D",
    "green": "#9ECE6A", "red": "#F7768E", "yellow": "#E0AF68", "mauve": "#BB9AF7", "orange": "#FF9E64", "cyan": "#7DCFFF",
}
# Presentation colours of the canvas (captions, sections, desktop scenes), not of the plugin.
CANVAS = {"caption": "#E6E9F5", "section": "#16161E", "desktop": "#13141C", "desktop-dark": "#0F0F14"}
BG, RAIL, CARD, LINE, BORDER = (TOKENS[k] for k in ("bg", "rail", "card", "line", "border"))
TEXT, DIM, FAINT = TOKENS["text"], TOKENS["text-dim"], TOKENS["text-faint"]
ACCENT, SEL, NAVSEL, HOVER, SURFACE = (TOKENS[k] for k in ("accent", "selected", "nav-selected", "hover", "surface"))
GREEN, RED, YELLOW, MAUVE, ORANGE, CYAN = (TOKENS[k] for k in ("green", "red", "yellow", "mauve", "orange", "cyan"))
VAR_FOR_HEX = {v.upper(): "color/" + k for k, v in TOKENS.items()}
VAR_FOR_HEX.update({v.upper(): "canvas/" + k for k, v in CANVAS.items()})
COLLECTION, MODE = "Tokens", "Tokyo Night"


def token_paint(field, value):
    """("fill", "#7AA2F71F") -> ({"fills": [paint with opacity]}, "color/accent"); (None, None) for non-tokens.

    A bound variable replaces the whole colour including alpha, so a tint keeps its alpha as paint opacity."""
    v = str(value or "").upper()
    var = VAR_FOR_HEX.get(v[:7])
    if not var or len(v) not in (7, 9):
        return None, None
    if len(v) == 7:
        return {field: value}, var
    return {field + "s": [{"type": "solid", "color": v[:7], "opacity": round(int(v[7:], 16) / 255, 3)}]}, var

# (size, bold) → text style name.
TEXT_STYLES = {
    (22, True): "Heading", (18, True): "Value", (14, False): "Icon",
    (13, False): "Body", (13, True): "Body/Bold", (12, False): "Small", (12, True): "Small/Bold",
    (11, False): "Caption", (11, True): "Caption/Bold", (10, False): "Micro", (10, True): "Micro/Bold",
}

# Nerd Font glyphs, as in plugin/Icons.qml.
G = {k: chr(v) for k, v in dict(
    shield=0xf0565, shieldOff=0xf099e, shieldAlert=0xf0ecb, overview=0xf056e, problems=0xf0026, rules=0xf0756,
    proxies=0xf0317, profiles=0xf0328, connections=0xf1616, traffic=0xf0127, logs=0xf021a, settings=0xf0493, search=0xf0349,
    hide=0xf0209, show=0xf0208, seen=0xf0139, back=0xf004d, route=0xf062c, check=0xf012c, refresh=0xf0450, undo=0xf054c,
    history=0xf02da, add=0xf0415, orphan=0xf0338, test=0xf140b, expanded=0xf0140, collapsed=0xf0142, info=0xf02fc, app=0xf08c6,
    path=0xf046d, remove=0xf01b4, arrow=0xf0054, globe=0xf059f, close=0xf0156, down=0xf01da, up=0xf0552, copy=0xf018f, edit=0xf03eb,
    file=0xf0224, statusCheck=0xf05e0, statusOff=0xf015a, unplug=0xf09a6, terminal=0xf018d, disk=0xf02ca, play=0xf040a, ip=0xf0a60,
    cpu=0xf061a, plug=0xf06a5, sets=0xf03d6, logic=0xf062d, sparkles=0xf0674, pulse=0xf0430, account=0xf0004, ban=0xf073a,
    timer=0xf051b, ruleAdd=0xf0412, lock=0xf033e, lockOpen=0xf0fc6).items()}

IDS_FILE = HERE / "ids.json"


def ids():
    """Component name → id, from ids.json (refresh with `ids.py`)."""
    return json.loads(IDS_FILE.read_text())["components"]


def C(name):
    return ids()[name]


def find(node, name):
    """First layer named `name` in a get_node tree (depth-first)."""
    if node.get("name") == name:
        return node
    for c in node.get("children") or []:
        r = find(c, name)
        if r:
            return r


class B:
    """Commands for one batch. Refs are {"$step": n}; layers bind tokens and text styles as they are made."""

    def __init__(self, bind=True):
        self.cmds, self.bind = [], bind
        self.ovs = []   # (ref, child name, node.set props | {"__inst": props} | {"__slot": fn})
        self.navs = []  # (rail ref, active label, {label: badge})

    def add(self, command, params):
        self.cmds.append({"command": command, "params": params})
        return {"$step": len(self.cmds) - 1}

    def _paints(self, props):
        """Turns token tints into paint opacity; returns {field: variable} to bind."""
        bind = {}
        for field in ("fill", "stroke"):
            paint, var = token_paint(field, props.get(field))
            if var and self.bind:
                props.pop(field)
                props.update(paint)
                bind[field] = var
        return bind

    def _bind(self, ref, bind):
        for field, var in bind.items():
            self.add("variable.bind", {"ids": [ref], "field": field, "variable": var})

    def node(self, type, parent=None, name=None, w=None, h=None, x=None, y=None, text=None, path=None, **props):
        p = {"type": type, "select": False}
        for k, v in (("parent", parent), ("name", name), ("width", w), ("height", h), ("x", x), ("y", y), ("text", text), ("path", path)):
            if v is not None:
                p[k] = v
        bind = self._paints(props)
        if props:
            p["props"] = props
        ref = self.add("node.create", p)
        self._bind(ref, bind)
        return ref

    def frame(self, parent, name, layout=None, w=None, h=None, fill=None, **props):
        props.setdefault("fill", fill or "none")
        if layout:
            props["layout"] = layout
            props.setdefault("sizingH", "hug" if w is None else "fixed")
            props.setdefault("sizingV", "hug" if h is None else "fixed")
        return self.node("frame", parent, name, w if w is not None else 10, h if h is not None else 10, **props)

    def row(self, parent, name="Row", gap=8, **kw):
        return self.frame(parent, name, "horizontal", gap=gap, crossAlign=kw.pop("crossAlign", "center"), **kw)

    def col(self, parent, name="Column", gap=4, **kw):
        return self.frame(parent, name, "vertical", gap=gap, **kw)

    def text(self, parent, s, size=13, color=TEXT, bold=False, name=None, w=None, **props):
        props.update(fontFamily=FONT, fontStyle="Bold" if bold else "Regular", fontSize=size, fill=color)
        if w == "fill":
            props.setdefault("sizingH", "fill")
            props.setdefault("textAutoResize", "height")
        ref = self.node("text", parent, name or s[:40], w if isinstance(w, (int, float)) else None, text=s, **props)
        style = TEXT_STYLES.get((size, bold))
        if self.bind and style and "letterSpacing" not in props and "lineHeight" not in props:
            self.add("style.apply", {"ids": [ref], "style": style})
        return ref

    def rect(self, parent, name, w, h, fill, **props):
        return self.node("rectangle", parent, name, w, h, fill=fill, **props)

    def ellipse(self, parent, name, d, fill, **props):
        return self.node("ellipse", parent, name, d, d, fill=fill, **props)

    def inst(self, parent, comp, props=None, **setp):
        p = {"component": C(comp) if isinstance(comp, str) else comp}
        if parent is not None:
            p["parent"] = parent
        r = self.add("instance.create", p)
        if props:
            self.add("instance.setProperties", {"ids": [r], "properties": props})
        if setp:
            self.set(r, **setp)
        return r

    def set(self, ref, **props):
        bind = self._paints(props)
        self.add("node.set", {"ids": [ref], "props": props})
        self._bind(ref, bind)

    # Changes inside instances made in this batch; applied by run() once ids are known.
    def ov(self, ref, child, **props):
        self.ovs.append((ref, child, props))

    def ovp(self, ref, child, props):
        self.ovs.append((ref, child, {"__inst": props}))

    def slot(self, ref, fn, name="Control"):
        """fn(b, slot_id) fills the slot frame `name` of instance `ref`."""
        self.ovs.append((ref, name, {"__slot": fn}))


def run(fc, b, label):
    """Send the batch, then the overrides, navigation states and slot contents it recorded."""
    res = fc.batch(b.cmds, label)
    rid = lambda ref: res[ref["$step"]]["id"]
    trees, sets, slots = {}, [], B(bind=b.bind)
    for ref, child, props in b.ovs:
        i = rid(ref)
        if i not in trees:
            trees[i] = fc.node(i, 8)
        n = find(trees[i], child)
        if n is None:
            print("override miss:", child)
        elif "__inst" in props:
            fc.cmd("instance.setProperties", ids=[n["id"]], properties=props["__inst"])
        elif "__slot" in props:
            props["__slot"](slots, n["id"])
        else:
            bind = b._paints(props)
            sets.append({"command": "node.set", "params": {"ids": [n["id"]], "props": props}})
            sets += [{"command": "variable.bind", "params": {"ids": [n["id"]], "field": f, "variable": v}} for f, v in bind.items()]
    for ref, active, badges in b.navs:
        rail = fc.node(rid(ref), 1)
        for c in rail["children"]:
            nav = (c.get("componentProperties") or {}).get("Label")
            if nav == active or nav in badges:
                fc.cmd("instance.setProperties", ids=[c["id"]], properties={"State": "Active" if nav == active else "Badge"})
            if nav in badges and nav != active:
                count = find(fc.node(c["id"], 3), "Count")
                if count:
                    sets.append({"command": "node.set", "params": {"ids": [count["id"]], "props": {"characters": str(badges[nav])}}})
    if sets:
        fc.batch(sets, label + " · overrides")
    if slots.cmds:
        fc.batch(slots.cmds, label + " · slots")
    return res


def make_set(fc, set_name, parent, variants, props=()):
    """Component set from variants [(\"Prop=Value, ...\", fn(b, parent, name) -> ref)].

    props: [(name, "text"|"boolean", layer name, default)] bound to that layer in every variant.
    """
    b = B()
    refs = [fn(b, parent, vname) for vname, fn in variants]
    res = fc.batch(b.cmds, set_name)
    vids = [res[r["$step"]]["id"] for r in refs]
    sid = fc.cmd("component.combineVariants", ids=vids)["id"] if len(vids) > 1 else vids[0]
    fc.tool("set_properties", {"ids": [sid], "props": {"name": set_name}})
    if props:
        trees = [fc.node(i, 6) for i in vids]
        for pname, ptype, layer, default in props:
            kw = dict(id=sid, name=pname, type=ptype, layer=find(trees[0], layer)["id"])
            if default is not None:
                kw["default"] = default
            fc.cmd("component.addProperty", **kw)
        binds = [{"command": "component.bindProperty", "params": {"layer": lay["id"], "property": pname,
                                                                   "field": "characters" if ptype == "text" else "visible"}}
                 for t in trees[1:] for pname, ptype, layer, _ in props for lay in [find(t, layer)] if lay]
        if binds:
            fc.batch(binds, set_name + " · properties")
    return sid, vids


# ---- UI helpers: instances of the design system's components ----

def btn(b, parent, label=None, icon=None, kind="Default", state="Default", small=False, **setp):
    p = {"Kind": kind, "State": state}
    p.update({"Label": label} if label is not None else {"Show label": False})
    p.update({"Icon": G.get(icon, icon)} if icon is not None else {"Show icon": False})
    if small:
        setp.update(width=24, height=24, padding=0, mainAlign="center", sizingH="fixed", sizingV="fixed")
    return b.inst(parent, "Btn", p, **setp)


def chip(b, parent, label, dot=False, selected=False):
    p = {"Label": label, "Selected": "Yes" if selected else "No"}
    if not dot:
        p["Show dot"] = False
    return b.inst(parent, "Chip", p)


def field(b, parent, text, state="Empty", icon=None, w=None):
    p = {"State": state, "Text": text}
    p.update({"Icon": G.get(icon, icon)} if icon else {"Show icon": False})
    return b.inst(parent, "Field", p, **({"sizingH": "fill"} if w == "fill" else ({"width": w} if w else {})))


def seg(b, parent, labels, current, name="Seg"):
    s = b.frame(parent, name, "horizontal", gap=2, padding=2, fill=CARD, stroke=LINE, strokeWeight=1, strokeAlign="inside", cornerRadius=6)
    for label in labels:
        b.inst(s, "SegOption", {"Selected": "Yes" if label == current else "No", "Label": label})
    return s


def switch(b, parent, on, compact=False):
    return b.inst(parent, "SwitchCompact" if compact else "Switch", {"On": "Yes" if on else "No"})


def keyhint(b, parent, key, label):
    return b.inst(parent, "KeyHint", {"Key": key, "Label": label})


def banner(b, parent, tone, text, action=None, icon=None):
    p = {"Tone": tone, "Text": text}
    p.update({"Action": action} if action else {"Show action": False})
    if icon:
        p["Icon"] = G.get(icon, icon)
    return b.inst(parent, "Banner", p, sizingH="fill")


def caption(b, parent, s):
    return b.text(parent, s, 11, FAINT, bold=True, letterSpacing=1)


def spacer(b, parent, name="Spacer"):
    return b.frame(parent, name, None, sizingH="fill", sizingV="fill")
