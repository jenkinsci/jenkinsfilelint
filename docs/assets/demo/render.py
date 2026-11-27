"""Render docs/assets/demo.gif: a terminal window with real jenkins-lint output.

Usage: python3 docs/assets/demo/render.py FONT_DIR docs/assets/demo.gif
jenkins-lint must be on PATH; FONT_DIR holds JetBrainsMono-Regular.ttf and -Bold.ttf.
Use a Pillow with raqm (features.check("raqm")), as Homebrew's, for the same text layout.
"""

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
FONTS = Path(sys.argv[1])
OUT = Path(sys.argv[2])
APP = Path(tempfile.mkdtemp()) / "demo"

W, H = 1388, 876  # 26 rows
PAD_X, TOP = 34, 70
SIZE, LINE = 21, 31
COLS = 104
INDENT = 4

BG = (13, 17, 23)
BAR = (22, 27, 34)
BORDER = (48, 54, 61)
FG = (230, 237, 243)
DIM = (125, 133, 144)
BLUE = (88, 166, 255)
GREEN = (63, 185, 80)
RED = (248, 81, 73)
ORANGE = (210, 153, 34)
PURPLE = (188, 140, 255)
CYAN = (57, 197, 207)

regular = ImageFont.truetype(str(FONTS / "JetBrainsMono-Regular.ttf"), SIZE)
bold = ImageFont.truetype(str(FONTS / "JetBrainsMono-Bold.ttf"), SIZE)
title_font = ImageFont.truetype(str(FONTS / "JetBrainsMono-Regular.ttf"), 17)
CW = regular.getlength("m")

SCENES = [
    (
        "# Check a directory: syntax, Declarative structure and security",
        "jenkins-lint .",
    ),
    ("# Preview the fixes", "jenkins-lint --diff --unsafe-fixes ."),
    ("# Apply them", "jenkins-lint --fix --unsafe-fixes --show-fixes ."),
]

SEVERITY = {
    "error": RED,
    "warning": ORANGE,
    "high": RED,
    "medium": ORANGE,
    "low": BLUE,
    "fixed": GREEN,
}
LOCATION = re.compile(
    r"^(\S+:\d+:\d+): (error|warning|high|medium|low|fixed): (?:([a-z-]+): )?(.*)$"
)


HEADER = re.compile(r"^(error|warning|high|medium|low)(?:\[([a-z-]+)\])?: (.*)$")
GUTTER = re.compile(r"^(\s*\d*\s*\|)(.*)$")
current = [RED]


def spans(line):
    """A line of output as (text, colour, bold) spans."""
    h = HEADER.match(line)
    if h:
        sev, rule, msg = h.groups()
        current[0] = SEVERITY[sev]
        out = [(sev, SEVERITY[sev], True)]
        if rule:
            out += [("[", FG, True), (rule, PURPLE, True), ("]", FG, True)]
        return out + [(": ", FG, False), (msg, FG, False)]
    if line.lstrip().startswith("--> "):
        i = line.index("-->")
        return [(line[: i + 4], BLUE, True), (line[i + 4 :], FG, False)]
    if re.match(r"^\s*= help: ", line):
        i = line.index("= help: ") + 8
        return [(line[:i], BLUE, True), (line[i:], BLUE, False)]
    g = GUTTER.match(line)
    if g:
        gutter, rest = g.groups()
        marks = re.match(r"^(\s*)([\^]+|[-]+)(.*)$", rest)
        if marks:
            pad, mark, label = marks.groups()
            colour = current[0] if mark[0] == "^" else BLUE
            return [
                (gutter, BLUE, True),
                (pad, FG, False),
                (mark + label, colour, True),
            ]
        return [(gutter, BLUE, True), (rest, FG, False)]
    m = LOCATION.match(line)
    if m:
        loc, sev, rule, msg = m.groups()
        out = [(loc + ": ", DIM, False), (sev, SEVERITY[sev], True), (": ", FG, False)]
        if rule and sev != "error":
            out += [(rule, PURPLE, True), (": ", FG, False)]
        elif rule:
            msg = rule + ": " + msg
        return out + [(msg, FG, False)]
    if line.startswith("  help: "):
        return [("  help: ", DIM, False), (line[8:], BLUE, False)]
    if line.startswith(("Found ", "No fixes")) or re.match(r"^\d+ findings? \(", line):
        return [(line, FG, True)]
    if line.startswith(("--- ", "+++ ")):
        return [(line, FG, True)]
    if line.startswith("@@"):
        return [(line, CYAN, False)]
    if line.startswith("-"):
        return [(line, RED, False)]
    if line.startswith("+"):
        return [(line, GREEN, False)]
    return [(line, FG, False)]


def wrap(span_list):
    """Wrap spans at COLS, at spaces, with a hanging indent."""
    rows, row, col = [], [], 0
    for text, colour, is_bold in span_list:
        for word in re.findall(r"\S+\s*|\s+", text):
            if col + len(word.rstrip()) > COLS and col > INDENT:
                rows.append(row)
                row, col = [(" " * INDENT, FG, False)], INDENT
            row.append((word, colour, is_bold))
            col += len(word)
    rows.append(row)
    return rows


def output_of(command):
    """stdout and stderr in the order the terminal shows them."""
    proc = subprocess.run(
        command + " 2>&1", shell=True, cwd=APP, capture_output=True, text=True
    )
    return proc.stdout.splitlines()


def frame(rows, cursor=False):
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W, 46], fill=BAR)
    d.line([0, 46, W, 46], fill=BORDER)
    for i, c in enumerate([(237, 106, 94), (245, 191, 79), (98, 197, 84)]):
        d.ellipse([18 + i * 24, 16, 32 + i * 24, 30], fill=c)
    tw = d.textlength("jenkins-lint", font=title_font)
    d.text(((W - tw) / 2, 13), "jenkins-lint", fill=DIM, font=title_font)
    y = TOP
    for row in rows[-((H - TOP) // LINE) :]:
        x = PAD_X
        for text, colour, is_bold in row:
            d.text((x, y), text, fill=colour, font=bold if is_bold else regular)
            x += CW * len(text)
        y += LINE
    if cursor:
        x = PAD_X + CW * sum(len(t) for t, _, _ in rows[-1])
        d.rectangle([x, y - LINE + 3, x + CW, y - 4], fill=FG)
    return img


PROMPT = [("~/demo", BLUE, True), (" $ ", DIM, False)]

frames, durations, scene_ends = [], [], []


def add(img, ms):
    frames.append(img)
    durations.append(ms)


for comment, command in SCENES:
    shutil.rmtree(APP, ignore_errors=True)
    shutil.copytree(HERE / "example", APP)
    rows = [[(comment, DIM, False)], list(PROMPT)]
    add(frame(rows, cursor=True), 700)
    for i in range(1, len(command) + 1):
        rows[-1] = PROMPT + [(command[:i], FG, True)]
        add(frame(rows, cursor=True), 45)
    add(frame(rows, cursor=True), 500)
    # Output longer than the window pauses on its first screen, then scrolls.
    first = True
    for line in output_of(command):
        more = wrap(spans(line))
        if first and len(rows) + len(more) > (H - TOP) // LINE:
            add(frame(rows), 1800)
            first = False
        rows += more
    rows.append(list(PROMPT))
    add(frame(rows, cursor=True), 4200)
    scene_ends.append(frames[-1])

shutil.rmtree(APP.parent, ignore_errors=True)
sample = Image.new("RGB", (W, H * len(scene_ends)))
for i, f in enumerate(scene_ends):
    sample.paste(f, (0, H * i))
palette = sample.quantize(colors=96, method=Image.Quantize.MEDIANCUT)
quantized = [f.quantize(palette=palette, dither=Image.Dither.NONE) for f in frames]
quantized[0].save(
    OUT,
    save_all=True,
    append_images=quantized[1:],
    duration=durations,
    loop=0,
    optimize=True,
)
print(OUT, len(frames), "frames", sum(durations) / 1000, "s")
