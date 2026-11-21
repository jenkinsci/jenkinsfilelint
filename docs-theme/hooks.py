"""Build-time markup that makes the documentation site look like Jenkins.

The Markdown in docs/audit stays plain, so it reads the same on GitHub. On the
site, the rules become a Jenkins job list with build status icons, and the
side panel gets the symbols of Jenkins tasks.
"""

import html as escape
import re
from pathlib import Path

SYMBOLS = Path(__file__).parent / "symbols"
STATUS = {"high": "status-red", "medium": "status-yellow", "low": "status-nobuilt"}
TASKS = {
    "Dashboard": "list",
    "Install": "download",
    "Usage": "terminal",
    "Pre-commit hook": "code-working",
    "Configuration": "settings",
    "Security": "lock-closed",
    "How it works": "help-circle",
    "Security audit": "shield-warning",
}
# Sumi wraps each table in a scrolling frame, which is kept for the new table.
TABLE = re.compile(
    r'(?:<div class="sumi-table[^"]*">\s*)?<table>.*?</table>(?:\s*</div>)?', re.DOTALL
)
INDEX_ROW = re.compile(r"^\| \[([a-z-]+)\]\([a-z-]+\.md\) \| (.+) \|$", re.MULTILINE)
META_ROW = re.compile(
    r"^\| Severity \| Confidence \| Persona \|\n\|[-|]+\n\| (.+) \| (.+) \| (.+) \|$",
    re.MULTILINE,
)
LEVEL = re.compile(
    r"^(high|medium|low|regular|pedantic|auditor)\b:?\s*(.*)$", re.DOTALL
)
CODE = re.compile(r"`([^`]+)`")
SIDEBAR = re.compile(r'<aside class="sumi-sidebar.*?</aside>', re.DOTALL)
NAV_LINK = re.compile(r"(<a\b[^>]*>)([^<]+)(</a>)")
H1 = re.compile(r"(<h1 [^>]*>)")
TITLE = re.compile(r"<title>.*?</title>", re.DOTALL)


def symbol(name: str) -> str:
    svg = (SYMBOLS / f"{name}.svg").read_text(encoding="utf-8")
    svg = TITLE.sub("", svg[svg.index("<svg") :]).strip()
    return f'<span class="jl-symbol" aria-hidden="true">{svg}</span>'


def inline(text: str) -> str:
    return CODE.sub(
        lambda m: f"<code>{m.group(1)}</code>", escape.escape(text, quote=False)
    )


def levels(cell: str) -> list:
    """Each `level: note` of a cell such as `high: webhooks; medium: parameters`."""
    parts = []
    for part in cell.split(";"):
        match = LEVEL.match(part.strip())
        parts.append(match.groups() if match else ("", part.strip()))
    return parts


def words(cell: str) -> str:
    return ", ".join(level for level, _ in levels(cell) if level)


def meta(docs_dir: str, rule: str) -> tuple:
    text = (Path(docs_dir) / "audit" / f"{rule}.md").read_text(encoding="utf-8")
    return META_ROW.search(text).groups()


def rule_table(docs_dir: str, prefix: str) -> str:
    index = (Path(docs_dir) / "audit" / "README.md").read_text(encoding="utf-8")
    rows = []
    for rule, finds in INDEX_ROW.findall(index):
        severity, confidence, persona = meta(docs_dir, rule)
        worst = levels(severity)[0][0]
        rows.append(
            f'<tr><td class="jl-status" title="{worst}">{symbol(STATUS[worst])}</td>'
            f'<td><a class="jl-job" href="{prefix}{rule}/">{rule}</a>'
            f'<span class="jl-job-note">{inline(finds)}</span></td>'
            f"<td>{words(severity)}</td>"
            f'<td class="jl-optional">{words(confidence)}</td>'
            f'<td class="jl-optional">{words(persona)}</td></tr>'
        )
    return (
        '<div class="sumi-table"><table class="jl-jobs"><thead><tr>'
        '<th aria-label="Status"></th><th>Rule</th><th>Severity</th>'
        '<th class="jl-optional">Confidence</th><th class="jl-optional">Persona</th>'
        f"</tr></thead><tbody>{''.join(rows)}</tbody></table></div>"
    )


def properties(docs_dir: str, rule: str) -> str:
    rows = []
    for name, cell in zip(("Severity", "Confidence", "Persona"), meta(docs_dir, rule)):
        lines = []
        for level, note in levels(cell):
            icon = (
                symbol(STATUS[level]) if name == "Severity" and level in STATUS else ""
            )
            note = f' <span class="jl-job-note">{inline(note)}</span>' if note else ""
            lines.append(f'<span class="jl-level">{icon}{level}{note}</span>')
        rows.append(f'<tr><th scope="row">{name}</th><td>{"".join(lines)}</td></tr>')
    return f'<div class="sumi-table"><table class="jl-properties"><tbody>{"".join(rows)}</tbody></table></div>'


def on_page_content(html, page, config, files):
    path = page.file.src_uri
    if path == "index.md":
        return html.replace(
            "<!-- audit-rules -->", rule_table(config.docs_dir, "audit/")
        )
    if path == "audit/README.md":
        return TABLE.sub(lambda m: rule_table(config.docs_dir, ""), html, count=1)
    if path.startswith("audit/"):
        rule = Path(path).stem
        worst = levels(meta(config.docs_dir, rule)[0])[0][0]
        html = TABLE.sub(lambda m: properties(config.docs_dir, rule), html, count=1)
        return H1.sub(lambda m: m.group(1) + symbol(STATUS[worst]), html, count=1)
    return html


def on_post_page(output, page, config):
    def task(link):
        opening, text, closing = link.groups()
        name = TASKS.get(text.strip())
        if (
            not name
            and (Path(config.docs_dir) / "audit" / f"{text.strip()}.md").exists()
        ):
            name = STATUS[levels(meta(config.docs_dir, text.strip())[0])[0][0]]
        if not name:
            return link.group(0)
        return f"{opening}{symbol(name)}<span>{text}</span>{closing}"

    return SIDEBAR.sub(lambda m: NAV_LINK.sub(task, m.group(0)), output, count=1)
