"""Plugin window scaffolding: a 1080×720 frame with the Rail and an auto-layout page."""
from lib import *

W, H = 1080, 720
CAPTION = CANVAS["caption"]


def window(b, name, x, y, nav, badges=None):
    """Returns (window, page). `nav` is the active Rail label; badges {label: count} replace key hints."""
    w = b.frame(None, name, "horizontal", w=W, h=H, fill=BG, x=x, y=y, stroke=BORDER, strokeWeight=1, strokeAlign="inside",
                cornerRadius=10, clip=True, effects=[{"type": "dropShadow", "color": "#00000066", "offset": [0, 12], "radius": 32}])
    rail = b.inst(w, "Rail", None, sizingV="fill")
    b.navs.append((rail, nav, {"Проблемы": 3, "Правила": 1} if badges is None else badges))
    page = b.col(w, "Page", gap=14, padding=18, sizingH="fill", sizingV="fill")
    return w, page


def caption_text(b, x, y, s, w=W):
    """Canvas caption above a screen: "N · Title — note\\nsubtitle"."""
    return b.text(None, s, 18, CAPTION, name="Подпись", w=w, x=x, y=y - 86, lineHeight=28, textAutoResize="height")


def header(b, page, title, sub, name="Header"):
    h = b.row(page, name, gap=10, sizingH="fill")
    t = b.col(h, "Titles", gap=3, sizingH="fill")
    b.text(t, title, 22, TEXT, bold=True, name="Title")
    b.text(t, sub, 12, FAINT, name="Subtitle", w="fill")
    return h


def back_header(b, page, crumbs, title):
    h = b.row(page, "Header", gap=10, sizingH="fill")
    btn(b, h, icon="back")
    t = b.col(h, "Titles", gap=3, sizingH="fill")
    b.text(t, crumbs, 11, FAINT)
    b.text(t, title, 22, TEXT, bold=True)
    return h


def keys(b, page, items):
    r = b.row(page, "Keys", gap=14, sizingH="fill")
    for k, label in items:
        keyhint(b, r, k, label)
    return r


def setting(b, parent, title, hint, control=None, new=False, highlight=False):
    """SettingRow; control(b, slot_id) fills its Control slot."""
    kw = {"sizingH": "fill"}
    if highlight:
        kw.update(fill=TOKENS["highlight"], stroke=TOKENS["highlight-border"], strokeWeight=1, strokeAlign="inside")
    r = b.inst(parent, "SettingRow", {"Title": title, "Hint": hint, "New": new}, **kw)
    if control:
        b.slot(r, control)
    return r


def settings_head(b, page, tab):
    header(b, page, "Настройки", "Применяются поверх любого профиля")
    seg(b, page, ["Основные", "Автоматизация", "Уведомления", "Ядро", "DNS"], tab, name="Tabs")
