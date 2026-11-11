"""Screens 28–30: site checks («Проверка»): the grid with a card menu, the add dialog, the empty list."""
from screen import *

X = 9300
G.update(sites=chr(0xf04c5), dots=chr(0xf01d9), browser=chr(0xf059f))

# Name, host, glyph (stands for the favicon), its colour, state, latency, route
SITES = [
    ("YouTube", "youtube.com", 0xf05c3, RED, "Good", "48 мс", "Европа · NL-Amsterdam-02"),
    ("Telegram", "telegram.org", 0xf2c6, CYAN, "Good", "61 мс", "Европа · NL-Amsterdam-02"),
    ("Discord", "discord.com", 0xf066f, MAUVE, "Slow", "740 мс", "Европа · NL-Amsterdam-02"),
    ("Instagram", "instagram.com", 0xf02fe, ORANGE, "Fail", "нет ответа", "Европа · NL-Amsterdam-02"),
    ("ChatGPT", "chatgpt.com", 0xf06a9, GREEN, "Good", "112 мс", "США · US-NewYork-01"),
    ("Claude", "claude.ai", 0xf0674, ORANGE, "Testing", "проверка…", "США · US-NewYork-01"),
    ("GitHub", "github.com", 0xf02a4, TEXT, "Good", "74 мс", "DIRECT"),
    ("Google", "google.com", 0xf02ad, ACCENT, "Good", "35 мс", "DIRECT"),
    ("X", "x.com", 0xf0544, TEXT, "Good", "88 мс", "Европа · NL-Amsterdam-02"),
    ("Spotify", "spotify.com", 0xf04c7, GREEN, "Good", "95 мс", "Европа · NL-Amsterdam-02"),
    ("Яндекс", "ya.ru", "Я", RED, "Good", "12 мс", "DIRECT"),
    ("Wikipedia", "wikipedia.org", 0xf05ac, TEXT, "Idle", "—", "не проверен"),
]
FOCUS = "GitHub"


def site_card(b, parent, name, host, glyph, colour, state, latency, route, focus=False):
    c = b.inst(parent, "SiteCard", {"State": state, "Focus": "Yes" if focus else "No", "Icon": glyph if isinstance(glyph, str) else chr(glyph),
                                    "Name": name, "Host": host, "Latency": latency, "Route": route}, sizingH="fill")
    b.ov(c, "Icon", fill=colour)
    return c


def toolbar(b, page):
    t = b.row(page, "Toolbar", gap=14, sizingH="fill")
    seg(b, t, ["Свой порядок", "Задержка", "Имя"], "Свой порядок", name="Sort")
    spacer(b, t)
    for colour, s in ((GREEN, "8 доступны"), (YELLOW, "1 медленно"), (RED, "1 нет ответа")):
        r = b.row(t, "Count", gap=6)
        b.ellipse(r, "Dot", 6, colour)
        b.text(r, s, 11, DIM)
    return t


def grid_screen(b, name, y, focus=True):
    w, page = window(b, name, X, y, "Проверка")
    h = header(b, page, "Проверка сайтов", "12 сайтов · по правилам, как пойдёт трафик · проверено 2 мин назад")
    btn(b, h, "Добавить", "add")
    btn(b, h, "Проверить все", "test", kind="Primary")
    toolbar(b, page)
    g = b.frame(page, "Grid", "grid", sizingH="fill", gridColumns=3, columnTracks=["1fr", "1fr", "1fr"], gap=10, crossGap=10)
    for s in SITES:
        site_card(b, g, *s, focus=focus and s[0] == FOCUS)
    spacer(b, page)
    keys(b, page, [("←↑↓→", "выбор"), ("Enter", "проверить"), ("t", "проверить все"), ("n", "добавить"), ("e", "изменить"),
                   ("[ ]", "переместить"), ("Del", "удалить")])
    return w, page


def menu(b, w):
    m = b.col(w, "Menu", gap=0, padding=4, w=250, sizingH="fixed", fill=CARD, stroke=BORDER, strokeWeight=1, strokeAlign="inside",
              cornerRadius=8, absolute=True, x=420, y=362, effects=[{"type": "dropShadow", "color": "#00000066", "offset": [0, 8], "radius": 24}])
    for glyph, label, key, state in ((G["test"], "Проверить", "Enter", "Hover"), (G["edit"], "Изменить", "e", "Default"),
                                     (G["browser"], "Открыть в браузере", "o", "Default"), (G["copy"], "Копировать адрес", "Ctrl+C", "Default"),
                                     (G["remove"], "Удалить", "Del", "Danger")):
        b.inst(m, "MenuItem", {"State": state, "Glyph": glyph, "Label": label, "Key": key}, sizingH="fill")
    return m


def labeled(b, parent, label, hint=None):
    c = b.col(parent, label, gap=6, sizingH="fill")
    b.text(c, label, 12, DIM, bold=True)
    return c


def dialog(b, w):
    b.rect(w, "Scrim", W, H, "#0F0F14B3", absolute=True, x=0, y=0)
    d = b.col(w, "Dialog", gap=16, padding=20, w=460, sizingH="fixed", fill=BG, stroke=BORDER, strokeWeight=1, strokeAlign="inside",
              cornerRadius=10, absolute=True, x=(W - 460) // 2 + 95, y=120, effects=[{"type": "dropShadow", "color": "#00000080", "offset": [0, 16], "radius": 40}])
    b.text(d, "Новый сайт", 18, TEXT, bold=True)
    c = labeled(b, d, "Адрес")
    field(b, c, "https://claude.ai", state="Filled", icon="globe", w="fill")
    b.text(c, "Без схемы — https://. Проверяется ответ сервера: любой HTTP-код значит «доступен»", 11, FAINT, w="fill")
    c = labeled(b, d, "Название")
    field(b, c, "Claude", state="Filled", w="fill")
    c = labeled(b, d, "Значок")
    seg(b, c, ["Авто", "Ссылка", "Буква"], "Авто", name="Icon")
    b.text(c, "Авто — значок сайта (favicon), скачивается через прокси и хранится в кэше", 11, FAINT, w="fill")
    c = labeled(b, d, "Так будет выглядеть")
    site_card(b, c, "Claude", "claude.ai", 0xf0674, ORANGE, "Idle", "—", "не проверен")
    f = b.row(d, "Footer", gap=8, sizingH="fill", mainAlign="max")
    btn(b, f, "Отмена")
    btn(b, f, "Сохранить", "check", kind="Primary")


def empty_screen(b, y):
    w, page = window(b, "Проверка · пусто — 1080×720", X, y, "Проверка")
    h = header(b, page, "Проверка сайтов", "Список пуст")
    btn(b, h, "Добавить", "add")
    c = b.col(page, "Empty", gap=10, sizingH="fill", sizingV="fill", mainAlign="center", crossAlign="center")
    b.text(c, G["sites"], 40, FAINT, name="Glyph")
    b.text(c, "Пока ни одного сайта", 13, TEXT, bold=True)
    b.text(c, "Добавьте свои сайты или начните с набора: YouTube, Telegram, Discord, Instagram, ChatGPT, Claude, GitHub и другие",
           12, DIM, w=420, textAlignHorizontal="center", textAutoResize="height")
    r = b.row(c, "Actions", gap=8)
    btn(b, r, "Набор по умолчанию", "sparkles", kind="Primary")
    btn(b, r, "Добавить сайт", "add")
    keys(b, page, [("n", "добавить")])


def main():
    fc, b = FC(), B()
    caption_text(b, X, 0, "28 · Проверка сайтов + меню карточки\nКлик по карточке — задержка через тот маршрут, по которому пойдёт сайт; «Проверить все» — все сразу")
    w, _ = grid_screen(b, "Проверка — 1080×720", 0)
    menu(b, w)
    caption_text(b, X, 1000, "29 · Новый сайт / изменить\nАдрес, название (по умолчанию из адреса), значок: favicon, своя ссылка или буква")
    w, _ = grid_screen(b, "Проверка · новый сайт — 1080×720", 1000, focus=False)
    dialog(b, w)
    caption_text(b, X, 2000, "30 · Проверка — пусто\nПервый запуск или всё удалено: набор по умолчанию одним нажатием")
    empty_screen(b, 2000)
    run(fc, b, "Screen: Проверка")


if __name__ == "__main__":
    main()
