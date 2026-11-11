# FigCraft scripts

Tools for keeping the design system in `design.figcraft`. They drive a running FigCraft app (`figcraft --control 7979`) through `figcraft-cli mcp --attach`. Command results stay in the script, so an agent's context only sees what a script prints.

The FigCraft window must be on screen: while it is hidden the app does not answer, and `fc.py` waits up to a minute.

| File | Purpose |
|---|---|
| `fc.py` | MCP client: `FC().cmd()`, `.batch()`, `.node()`, `.doc()`, `.shot(id, path)` (PNG to a file) |
| `lib.py` | tokens (`TOKENS`, `G` glyphs, `TEXT_STYLES`), the batch builder `B` + `run()`, `make_set()`, helpers `btn/chip/field/seg/switch/banner/keyhint` |
| `screen.py` | plugin window scaffolding: `window()`, `header()`, `back_header()`, `setting()`, `keys()`, `caption_text()` |
| `tree.py` | compact layer tree: `tree.py`, `tree.py ID [DEPTH]`, `tree.py --find NAME` (exact name) |
| `ids.py` | refreshes `ids.json` (component name → id); run it after adding or renaming components |
| `tokens.py` | syncs colour variables and text styles with `lib.py`; `--bind` / `--styles` attach existing layers to them |
| `history.py` | last undo steps; `--wait N` blocks until nobody edited for N seconds |
| `selection.py` | the selected layers as `figcraft #ID TYPE «name» W×H — path` lines; `ID...` for given layers; `--copy` puts them on the clipboard (bound to SUPER+ALT+C) |
| `comments.py` | open comment threads with the layer under each pin; `--all`, `reply ID TEXT`, `resolve ID` |
| `present_copy.py`, `en_strings.py` | English copies of the screens on the page «Презентация» (`present_ids.json`) |
| `present.py` | the marketplace and README slides; `--export` writes `preview.png` and `docs/images/*.png` |

## Building something

```python
from screen import *

fc, b = FC(), B()
w, page = window(b, "Правила · список — 1080×720", 4100, 1000, "Правила")
h = header(b, page, "Мои правила", "8 правил · выше правил подписки")
btn(b, h, "Добавить", "add", kind="Primary")
r = b.inst(page, "RuleRow", {"State": "Selected", "Kind": "SUFFIX", "Value": "youtube.com"}, sizingH="fill")
b.ovp(r, "TargetPill", {"Tone": "Direct", "Target": "DIRECT"})  # nested instance, set after the batch
run(fc, b, "Screen: Rules")
```

Colours passed as the `lib.py` constants are bound to variables, and texts get the text style for their size and weight. A palette change is then `TOKENS` + `tokens.py`, not an edit of every layer.

## Remarks on the design

The user points at layers in two ways:

- **Selection.** They select layers and press SUPER+ALT+C (a Hyprland binding that runs `selection.py --copy`), then paste the `figcraft #ID …` lines into the chat with the remark, or just say "the selected one". The id is what to change; ◇ in the path marks an instance, and `main:` names the components to edit instead of overriding inside the instance.
- **Comments.** They leave comments in FigCraft (C). `comments.py` lists them; after a fix, answer with `comments.py reply ID "what changed"` and close it with `comments.py resolve ID`.
