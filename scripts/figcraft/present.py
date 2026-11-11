"""Step 2 of the presentation: slides on the page «Презентация» from the copies
made by present_copy.py, exported to preview.png and docs/images/*.png.

    present.py            build the slides (once) and export them
    present.py --export   export only (slides carry their PNG export settings)
"""
import json
import pathlib
import sys

from fc import FC
from lib import ACCENT, BORDER, CARD, DIM, FAINT, G, GREEN, MAUVE, TEXT, YELLOW, B, run

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
IDS = json.loads((HERE / "present_ids.json").read_text())
W, H = 1600, 1000
DEEP, NIGHT = "#1A1B26", "#0F0F14"

# name → (file, scale)
EXPORTS = {
    "Slide · Preview": ("preview.png", 2),
    "Slide · Problems": ("docs/images/problems.png", 1),
    "Slide · Rules": ("docs/images/rules.png", 1),
    "Slide · Traffic": ("docs/images/traffic.png", 1),
    "Slide · How it works": ("docs/images/how-it-works.png", 1),
}


def slide(b, name, i, glow=(1000, 560)):
    s = b.frame(None, name, w=W, h=H, x=i * (W + 200), y=0, clip=True)
    b.set(s, fills=[{"type": "linearGradient", "stops": [DEEP, NIGHT], "from": [0.5, 0], "to": [0.5, 1]}])
    g = b.ellipse(s, "Glow", 900, ACCENT, x=glow[0] - 450, y=glow[1] - 450, opacity=0.16)
    b.set(g, effects=[{"type": "layerBlur", "radius": 220}])
    return s


def heading(b, s, title, sub, y=64):
    b.text(s, title, 40, TEXT, True, name="Title", x=80, y=y)
    b.text(s, sub, 18, DIM, name="Subtitle", x=80, y=y + 64, w=1440, textAutoResize="height", lineHeight=28)


def shadow(fc, ids):
    fc.cmd("node.set", ids=ids, props={"effects": [{"type": "dropShadow", "color": "#0000008C", "offset": [0, 28], "radius": 72}]})


def place(fc, node, parent, x, y, scale=None):
    fc.cmd("node.reparent", ids=[node], parent=parent)
    if scale:
        fc.cmd("transform.scale", ids=[node], factor=scale)
    fc.cmd("node.set", ids=[node], props={"x": x, "y": y})


def preview(fc):
    b = B()
    s = slide(b, "Slide · Preview", 0, glow=(1060, 500))
    col = b.col(s, "Pitch", gap=28, x=80, y=0, w=500)
    b.text(col, G["shield"], 56, ACCENT, name="Shield")
    name = b.col(col, "Name", gap=0)
    b.text(name, "Mihomo", 64, TEXT, True)
    b.text(name, "for Omarchy", 64, ACCENT, True)
    b.text(col, "Proxies, rules and the sites that fail, one click away in the Omarchy bar.", 20, DIM,
           name="Tagline", w=480, textAutoResize="height", lineHeight=30)
    feats = b.col(col, "Features", gap=14)
    for glyph, color, label in ((G["problems"], YELLOW, "See what fails, route it in one key"),
                                (G["sparkles"], MAUVE, "Autofix tries your other groups"),
                                (G["rules"], ACCENT, "Your rules above the subscription"),
                                (G["traffic"], GREEN, "Traffic by app, site and country")):
        r = b.row(feats, "Feature", gap=14)
        b.text(r, glyph, 20, color, name="Icon")
        b.text(r, label, 16, TEXT, name="Label")
    res = run(fc, b, "Slide · Preview")
    sid, cid = res[s["$step"]]["id"], res[col["$step"]]["id"]
    win = next(c["id"] for c in fc.node(IDS["scene"], 1)["children"] if c["type"] == "INSTANCE")
    place(fc, win, sid, 600, 86, scale=0.68)
    shadow(fc, [win])
    h = fc.node(cid, 0)["height"]
    fc.cmd("node.set", ids=[cid], props={"y": round((H - h) / 2)})
    return sid


def screen_slide(fc, i, name, key, title, sub, extra=None):
    b = B()
    s = slide(b, name, i, glow=(800, 620))
    heading(b, s, title, sub)
    res = run(fc, b, name)
    sid = res[s["$step"]]["id"]
    place(fc, IDS[key], sid, 260, 232)
    shadow(fc, [IDS[key]])
    if extra:
        extra(fc, sid)
    return sid


def notice(fc, sid):
    place(fc, IDS["notice"], sid, 1190, 600)
    shadow(fc, [IDS["notice"]])
    b = B()
    tag = b.row(sid, "Autofix tag", gap=8, x=1190, y=552, padding=8, cornerRadius=14, fill=CARD, stroke=MAUVE)
    b.text(tag, G["sparkles"], 14, MAUVE, name="Icon")
    b.text(tag, "Autofix at work", 13, TEXT, True, name="Label")
    run(fc, b, "Slide · Problems · tag")


def box(b, parent, x, y, w, h, glyph, color, title, lines, highlight=False):
    f = b.col(parent, title, gap=12, x=x, y=y, w=w, h=h, padding=26, cornerRadius=16,
              fill=CARD, stroke=color if highlight else BORDER, strokeWeight=2 if highlight else 1)
    b.text(f, glyph, 34, color, name="Icon")
    b.text(f, title, 22, TEXT, True, name="Title")
    b.text(f, lines, 15, DIM, name="Body", w="fill", lineHeight=23)
    return f


def arrow(b, parent, x, y, gap, label):
    b.node("vector", parent, "Arrow", path="M0 6 L64 6 M55 0 L64 6 L55 12", x=x + (gap - 64) / 2, y=y, fill="none", stroke=FAINT, strokeWeight=2)
    b.text(parent, label, 12, FAINT, name="Arrow label", x=x, y=y - 28, w=gap, textAlignHorizontal="center")


def how(fc):
    b = B()
    s = slide(b, "Slide · How it works", 4, glow=(800, 420))
    heading(b, s, "How it fits together", "The plugin is the interface. A small daemon does the work, as your user, and runs the mihomo core.")
    xs, y, w, h = (80, 470, 860, 1250), 216, 270, 270
    box(b, s, xs[0], y, w, h, G["shield"], ACCENT, "Bar widget", "This plugin, in QML.\nRuns inside the\nOmarchy shell.", True)
    box(b, s, xs[1], y, w, h, G["cpu"], MAUVE, "omarchy-mihomod", "Rust daemon, a\nsystemd --user service.\nRules, autofix, stats.")
    box(b, s, xs[2], y, w, h, G["pulse"], GREEN, "mihomo core", "systemd --user service.\nTUN or the system\nproxy, as you choose.")
    box(b, s, xs[3], y, w, h, G["globe"], YELLOW, "Your proxies", "Subscriptions and\nyour own nodes:\nVLESS, Trojan, Hy2…")
    for i, label in enumerate(("Unix socket", "REST · config", "proxy protocols")):
        arrow(b, s, xs[i] + w, y + h // 2 - 6, xs[i + 1] - xs[i] - w, label)
    for i, (glyph, color, title, body) in enumerate((
            (G["lock"], GREEN, "Nothing runs as root", "TUN needs capabilities on the core binary:\none sudo setcap, only if you agree."),
            (G["disk"], ACCENT, "Your data stays on this machine", "Profiles, rules and statistics live in\n~/.local/share/omarchy-mihomo. No telemetry."))):
        c = b.col(s, title, gap=10, x=80 + i * 735, y=526, w=705, padding=24, cornerRadius=16, fill=CARD, stroke=BORDER)
        r = b.row(c, "Head", gap=12)
        b.text(r, glyph, 20, color, name="Icon")
        b.text(r, title, 18, TEXT, True, name="Title")
        b.text(c, body, 15, DIM, name="Body", w="fill", lineHeight=23)
    t = b.col(s, "Install", gap=12, x=80, y=708, w=1440, padding=28, cornerRadius=16, fill=NIGHT, stroke=BORDER)
    r = b.row(t, "Head", gap=12)
    b.text(r, G["terminal"], 20, MAUVE, name="Icon")
    b.text(r, "Install in two steps", 18, TEXT, True, name="Title")
    for comment, cmd in (("# 1. the widget", "omarchy plugin add https://github.com/vornashev/omarchy-mihomo --enable"),
                         ("# 2. the daemon and the core", "~/.config/omarchy/plugins/vornashev.omarchy-mihomo/packaging/install.sh")):
        b.text(t, comment, 15, FAINT, name="Comment")
        b.text(t, "$ " + cmd, 15, GREEN, name="Command")
    res = run(fc, b, "Slide · How it works")
    return res[s["$step"]]["id"]


def build(fc):
    fc.cmd("page.select", id=IDS["page"])
    preview(fc)
    screen_slide(fc, 1, "Slide · Problems", "problems", "See what fails. Route it in one key.",
                 "Timeouts, resets and DNS sinkholes, grouped by site and by the rule that sent them. "
                 "Autofix tries your other groups and keeps a route only while it works.", notice)
    screen_slide(fc, 2, "Slide · Rules", "rules", "Your rules, above the subscription's",
                 "By domain, app, path or IP, with rule sets from the MetaCubeX catalog. "
                 "Every change goes into a history you can undo, and subscription updates keep them.")
    screen_slide(fc, 3, "Slide · Traffic", "traffic", "Where your traffic goes",
                 "Every app, site, country and node over 30 days. Click a row to narrow down, "
                 "drag over the chart to zoom, press r to make a rule.")
    how(fc)


def export(fc):
    """Each slide's PNG export setting, written into the repository under its EXPORTS path."""
    import shutil
    import tempfile
    fc.cmd("page.select", id=IDS["page"])
    tops = {c["name"]: c["id"] for c in fc.tops()}
    tmp = tempfile.mkdtemp()
    for f in fc.cmd("file.exportLayers", ids=[tops[n] for n in EXPORTS], dir=tmp)["files"]:
        name = next(n for n in EXPORTS if tops[n] == f["id"])
        dest = REPO / EXPORTS[name][0]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(f["path"], dest)
        print(dest.relative_to(REPO), f["bytes"] // 1024, "KB")
    shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    fc = FC()
    if "--export" not in sys.argv:
        build(fc)
    export(fc)
