#!/usr/bin/env python3
"""Generate demo.gif for the README.

Renders a scripted terminal session (pre-commit hook catching a Jenkinsfile
syntax error, fixing it in vi, and committing successfully) as an animated
GIF. Only dependency is Pillow:

    pip install pillow
    python scripts/generate_demo_gif.py
"""

from __future__ import annotations

import pathlib

from PIL import Image, ImageDraw, ImageFont

# ---------------------------------------------------------------- geometry
COLS = 100
ROWS = 27
FONT_SIZE = 24
PAD_X = 24
PAD_Y = 18

FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_BOLD_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"

FONT = ImageFont.truetype(FONT_PATH, FONT_SIZE)
FONT_BOLD = ImageFont.truetype(FONT_BOLD_PATH, FONT_SIZE)
CHAR_W = FONT.getlength("M")
LINE_H = int(FONT_SIZE * 1.32)

WIDTH = int(COLS * CHAR_W + 2 * PAD_X)
HEIGHT = ROWS * LINE_H + 2 * PAD_Y

# ------------------------------------------------------------------ colors
BG = (24, 25, 27)
FG = (214, 214, 214)
GRAY = (120, 120, 122)
WHITE = (245, 245, 245)
RED_BG = (205, 49, 49)
GREEN_BG = (13, 188, 121)
BLUE = (106, 135, 189)
CURSOR = (200, 202, 205)

PROMPT = "sxp@Mac demo % "

# A segment is (text, fg, bg-or-None, bold). A line is a list of segments.
def seg(text, fg=FG, bg=None, bold=False):
    return (text, fg, bg, bold)


def render(lines, cursor=None):
    """Render terminal lines to an image. cursor is (row, col) or None."""
    img = Image.new("RGB", (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(img)
    for row, line in enumerate(lines[:ROWS]):
        x = PAD_X
        y = PAD_Y + row * LINE_H
        for text, fg, bg, bold in line:
            w = CHAR_W * len(text)
            if bg is not None:
                draw.rectangle([x, y, x + w, y + LINE_H - 1], fill=bg)
            draw.text((x, y + 2), text, font=FONT_BOLD if bold else FONT, fill=fg)
            x += w
    if cursor is not None:
        row, col = cursor
        x = PAD_X + col * CHAR_W
        y = PAD_Y + row * LINE_H
        draw.rectangle([x, y + 1, x + CHAR_W - 1, y + LINE_H - 2], fill=CURSOR)
    return img


class Recorder:
    def __init__(self):
        self.frames = []
        self.durations = []
        self.lines = []

    def snap(self, duration, cursor="eol"):
        if cursor == "eol":
            cursor = self._eol()
        self.frames.append(render(self.lines, cursor))
        self.durations.append(duration)

    def _eol(self):
        if not self.lines:
            return (0, 0)
        row = len(self.lines) - 1
        col = sum(len(s[0]) for s in self.lines[row])
        return (row, col)

    def add_line(self, *segments):
        self.lines.append(list(segments))

    def type_text(self, text, chars_per_frame=2, delay=70):
        """Append text to the last line with a typing animation."""
        for i in range(0, len(text), chars_per_frame):
            chunk = text[i : i + chars_per_frame]
            last = self.lines[-1]
            if last and last[-1][1:] == (FG, None, False):
                last[-1] = seg(last[-1][0] + chunk)
            else:
                last.append(seg(chunk))
            self.snap(delay)


def hook_line(result, bg):
    dots = "." * (79 - len("jenkinsfilelint") - len(result))
    return [seg("jenkinsfilelint" + dots), seg(result, WHITE, bg)]


BROKEN_JENKINSFILE = [
    "pipeline {",
    "    agent",
    "    stages {",
    "        stage('Build') {",
    "            steps {",
    "                script {",
    "                    // Run the build process",
    "                    sh 'echo Building...'",
    "                }",
    "            }",
    "        }",
    "    }",
    "}",
]

FAIL_OUTPUT = [
    [seg("- hook id: jenkinsfilelint", GRAY)],
    [seg("- exit code: 1", GRAY)],
    [],
    [seg("Errors encountered validating Jenkinsfile:")],
    # terminal hard-wrap of one long error line at COLS characters
    [seg('WorkflowScript: 2: Not a valid section definition: "agent". Some extra configuration is required. @ ')],
    [seg('line 2, column 5.')],
    [seg("       agent")],
    [seg("       ^")],
    [],
    [seg('WorkflowScript: 1: Missing required section "agent" @ line 1, column 1.')],
    [seg("   pipeline {")],
    [seg("   ^")],
    [],
]

SUCCESS_OUTPUT = [
    [seg("[main (root-commit) 66d1e3e] Add Jenkinsfile")],
    [seg(" 1 file changed, 13 insertions(+)")],
    [seg(" create mode 100644 Jenkinsfile")],
]


def build():
    rec = Recorder()

    # -- first (failing) commit attempt
    rec.add_line(seg(PROMPT))
    rec.snap(1200)
    rec.type_text("git add Jenkinsfile")
    rec.snap(350)
    rec.add_line(seg(PROMPT))
    rec.type_text('git commit -m "Add Jenkinsfile"')
    rec.snap(600)

    rec.lines.append(hook_line("Failed", RED_BG))
    rec.snap(700, cursor=None)
    rec.lines.extend(FAIL_OUTPUT)
    rec.add_line(seg(PROMPT))
    rec.snap(2600)

    # -- fix the file in vi
    rec.type_text("vi Jenkinsfile")
    rec.snap(500)

    shell_lines = [list(l) for l in rec.lines]
    vi = Recorder()
    vi.frames, vi.durations = rec.frames, rec.durations
    vi.lines = [[seg(l)] for l in BROKEN_JENKINSFILE]
    vi.lines += [[seg("~", BLUE)] for _ in range(ROWS - len(BROKEN_JENKINSFILE) - 1)]
    vi.lines.append([seg('"Jenkinsfile" 13L, 248B')])
    agent_row, agent_col = 1, len("    agent")
    vi.snap(900, cursor=(agent_row, agent_col - 1))

    vi.lines[-1] = [seg("-- INSERT --", WHITE, None, True)]
    vi.snap(500, cursor=(agent_row, agent_col))
    for i, ch in enumerate(" none"):
        vi.lines[agent_row] = [seg("    agent" + " none"[: i + 1])]
        vi.snap(110, cursor=(agent_row, agent_col + i + 1))
    vi.snap(450, cursor=(agent_row, agent_col + 5))

    vi.lines[-1] = [seg(":wq")]
    vi.snap(600, cursor=(len(vi.lines) - 1, 3))

    # -- back to the shell, second (passing) commit
    rec.frames, rec.durations = vi.frames, vi.durations
    rec.lines = shell_lines
    rec.add_line(seg(PROMPT))
    rec.snap(600)
    rec.type_text("git add Jenkinsfile")
    rec.snap(350)
    rec.add_line(seg(PROMPT))
    rec.type_text('git commit -m "Add Jenkinsfile"')
    rec.snap(600)

    rec.lines.append(hook_line("Passed", GREEN_BG))
    rec.snap(500, cursor=None)
    rec.lines.extend(SUCCESS_OUTPUT)
    rec.add_line(seg(PROMPT))
    rec.snap(1600)

    rec.type_text("exit")
    rec.snap(3500)
    return rec


def main():
    rec = build()
    out = pathlib.Path(__file__).resolve().parent.parent / "demo.gif"
    frames = [f.quantize(colors=64, dither=Image.Dither.NONE) for f in rec.frames]
    frames[0].save(
        out,
        save_all=True,
        append_images=frames[1:],
        duration=rec.durations,
        loop=0,
        optimize=True,
    )
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB, "
          f"{len(frames)} frames, {WIDTH}x{HEIGHT})")


if __name__ == "__main__":
    main()
