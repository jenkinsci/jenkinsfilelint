"""Build-time markup for the audit pages of the documentation site.

The Markdown in docs/audit stays plain, so it reads the same on GitHub. On the
site, the table at the top of a rule becomes a card with severity pills, the
list of rules becomes a grid of cards, and code examples are marked as flagged
or fixed.
"""

import re
from pathlib import Path

# Sumi wraps each table in a scrolling frame, which a card must not keep.
TABLE = re.compile(
    r'(?:<div class="sumi-table[^"]*">\s*)?<table>.*?</table>(?:\s*</div>)?', re.DOTALL
)
CELL = re.compile(r"<t[dh][^>]*>(.*?)</t[dh]>", re.DOTALL)
ROW = re.compile(r"<tr>(.*?)</tr>", re.DOTALL)
LINK = re.compile(r'<a href="([^"]+)">(.*?)</a>')
SECTION = re.compile(r'(?=<h2 id="[^"]+")')
LEVEL = re.compile(
    r"^(high|medium|low|regular|pedantic|auditor)\b:?\s*(.*)$", re.DOTALL
)
SOURCE_ROW = re.compile(r"^\| Severity \|.*\n\|[-| ]+\n\| ([^|]+) \|", re.MULTILINE)
SEVERITY = re.compile(r"\b(high|medium|low)\b(?=:|;|\s*$)")
CODE_CLASS = {"what-it-finds": "jl-code-flagged", "how-to-fix-it": "jl-code-fixed"}


def pills(cell: str) -> str:
    """A cell such as `high: pull requests; medium: parameters`, as pills."""
    lines = []
    for part in cell.split(";"):
        match = LEVEL.match(part.strip())
        if not match:
            lines.append(f"<span>{part.strip()}</span>")
            continue
        level, note = match.groups()
        note = f" <span>{note}</span>" if note else ""
        lines.append(
            f'<span><span class="jl-pill jl-{level}">{level}</span>{note}</span>'
        )
    return "".join(lines)


def rule_meta(table: str) -> str:
    rows = ROW.findall(table)
    names, values = CELL.findall(rows[0]), CELL.findall(rows[1])
    items = "".join(
        f"<div><dt>{name}</dt><dd>{pills(value)}</dd></div>"
        for name, value in zip(names, values)
    )
    return f'<dl class="jl-rule-meta">{items}</dl>'


def severities(docs_dir: str, rule: str) -> list:
    """The severities of a rule, from the table at the top of its page."""
    text = (Path(docs_dir) / "audit" / f"{rule}.md").read_text(encoding="utf-8")
    return list(dict.fromkeys(SEVERITY.findall(SOURCE_ROW.search(text).group(1))))


def rule_grid(table: str, docs_dir: str) -> str:
    cards = []
    for number, row in enumerate(ROW.findall(table)[1:], 1):
        rule_cell, finds = CELL.findall(row)
        href, name = LINK.search(rule_cell).groups()
        levels = severities(docs_dir, name)
        badges = "".join(f'<span class="jl-pill jl-{s}">{s}</span>' for s in levels)
        cards.append(
            f'<a class="jl-rule-card jl-edge-{levels[0]}" href="{href}">'
            f'<span class="jl-rule-top"><span class="jl-stage">{number:02d}</span>{badges}</span>'
            f"<strong>{name}</strong><span>{finds}</span></a>"
        )
    return f'<div class="jl-rule-grid">{"".join(cards)}</div>'


def mark_code(html: str) -> str:
    parts = SECTION.split(html)
    for i, part in enumerate(parts):
        match = re.match(r'<h2 id="([^"]+)"', part)
        css = CODE_CLASS.get(match.group(1)) if match else None
        if css:
            parts[i] = part.replace(
                '<div class="highlight">', f'<div class="highlight {css}">'
            )
    return "".join(parts)


def on_page_content(html, page, config, files):
    path = page.file.src_uri
    if not path.startswith("audit/"):
        return html
    if path == "audit/README.md":
        return TABLE.sub(
            lambda m: rule_grid(m.group(0), config.docs_dir), html, count=1
        )
    return mark_code(TABLE.sub(lambda m: rule_meta(m.group(0)), html, count=1))
