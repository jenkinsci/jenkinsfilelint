"""Render the repository's social preview (Settings > Social preview), 1280x640.

Usage: python3 docs/assets/demo/social.py FONT_DIR social-preview.png
FONT_DIR holds Outfit-*.ttf and JetBrainsMono-*.ttf; Google Chrome draws the page.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
FONTS = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
LOGO = (HERE.parent / "jenkins.svg").as_uri()

CODE = [
    (7, "steps {", ""),
    (8, "  script {", ""),
    (9, '    def branch = "${env.CHANGE_BRANCH}"', ""),
    (10, '    sh "git checkout ${branch}"', "bad"),
    (11, "  }", ""),
    (12, "  withCredentials([string(...)]) {", ""),
    (13, "    sh 'npm publish --token \"$TOKEN\"'", ""),
]

L9 = '   def branch = "${env.CHANGE_BRANCH}"'
L10 = '   sh "git checkout ${branch}"'
READ = " " * L9.index("$") + "-" * len("${env.CHANGE_BRANCH}") + " read here"
CARET = " " * L10.index("$") + "^" * len("${branch}")
TERMINAL = f"""<span class=dim>$</span> jenkins-lint Jenkinsfile
<b class=red>high</b><b>[<span class=purple>script-injection</span>]</b>: CHANGE_BRANCH is the name of
the pull request's branch, and Groovy puts it into branch…
<b class=blue>  --&gt;</b> Jenkinsfile:10:38
<b class=blue> 9 |</b>{L9}
<b class=blue>   |</b><b class=blue>{READ}</b>
<b class=blue>10 |</b>{L10}
<b class=blue>   |</b><b class=red>{CARET}</b>
<span class=dim>1 finding (1 high)</span>"""

ICONS = {
    "shield": '<path d="M12 2l8 3v6c0 5-3.5 9.5-8 11-4.5-1.5-8-6-8-11V5z" fill="#e5383b"/>'
    '<path d="M9 9l6 6M15 9l-6 6" stroke="#fff" stroke-width="2.2" stroke-linecap="round"/>',
    "bolt": '<circle cx="12" cy="12" r="10" fill="#2f6fe4"/>'
    '<path d="M13 5l-5 8h4l-1 6 5-8h-4z" fill="#fff"/>',
    "check": '<circle cx="12" cy="12" r="10" fill="#2fb36b"/>'
    '<path d="M7.5 12.5l3 3 6-6.5" stroke="#fff" stroke-width="2.4" fill="none" stroke-linecap="round"/>',
    "file": '<rect x="5" y="2" width="14" height="20" rx="2" fill="none" stroke="#2f6fe4" stroke-width="1.8"/>'
    '<path d="M10 10l-2 2.5 2 2.5M14 10l2 2.5-2 2.5" stroke="#2f6fe4" stroke-width="1.8" fill="none"/>',
    "prompt": '<rect x="2" y="3" width="20" height="18" rx="3" fill="#1c2333"/>'
    '<path d="M6.5 9l3 3-3 3M11.5 15.5h5" stroke="#fff" stroke-width="1.8" fill="none" stroke-linecap="round"/>',
    "git": '<rect x="3" y="3" width="18" height="18" rx="4" transform="rotate(45 12 12)" fill="#2f6fe4"/>'
    '<path d="M12 7v10M12 12l3-3" stroke="#fff" stroke-width="1.8" fill="none"/>'
    '<circle cx="12" cy="7" r="1.6" fill="#fff"/><circle cx="12" cy="17" r="1.6" fill="#fff"/>'
    '<circle cx="15" cy="9" r="1.6" fill="#fff"/>',
}


def icon(name, size):
    return (
        f'<svg width="{size}" height="{size}" viewBox="0 0 24 24">{ICONS[name]}</svg>'
    )


code = "".join(
    f'<div class="ln {kind}"><span class=num>{"✕" if kind else n}</span>'
    f"<span>{text.replace('<', '&lt;')}</span></div>"
    for n, text, kind in CODE
)
features = "".join(
    f"<div class=feat>{icon(i, 44)}<span>{a}<br>{b}</span></div>"
    for i, a, b in [
        ("shield", "Syntax and", "structure"),
        ("bolt", "Offline, in", "milliseconds"),
        ("check", "Security rules", "and fixes"),
    ]
)
flow = "".join(
    f"<div class=step>{icon(i, 34)}<span>{a}<br>{b}</span></div>"
    + (f"<div class='arrow {k}'></div>" if k else "")
    for i, a, b, k in [
        ("file", "Edit", "Jenkinsfile", "solid"),
        ("prompt", "Pre-commit", "hook", "solid"),
        ("check", "<b>jenkins-lint</b>", "offline check", "dash"),
        ("git", "Commit", "with confidence", ""),
    ]
)

HTML = f"""<!doctype html><meta charset=utf-8>
<style>
@font-face {{ font-family: Outfit; src: url({(FONTS / "Outfit-Regular.ttf").as_uri()}); }}
@font-face {{ font-family: Outfit; font-weight: 700; src: url({(FONTS / "Outfit-Bold.ttf").as_uri()}); }}
@font-face {{ font-family: Mono; src: url({(FONTS / "JetBrainsMono-Regular.ttf").as_uri()}); }}
@font-face {{ font-family: Mono; font-weight: 700; src: url({(FONTS / "JetBrainsMono-Bold.ttf").as_uri()}); }}
* {{ box-sizing: border-box; margin: 0; }}
body {{ width: 1280px; height: 640px; overflow: hidden; font-family: Outfit; color: #121a2c;
  background: radial-gradient(circle at 80% 20%, #e9eefb, #f6f8fd 55%, #fff); position: relative; }}
.grid {{ position: absolute; inset: 0; background-image: radial-gradient(#d5dcec 1.2px, transparent 1.2px);
  background-size: 18px 18px; opacity: .45; }}
.brand {{ position: absolute; left: 42px; top: 46px; display: flex; align-items: center; gap: 14px;
  font-size: 26px; font-weight: 700; }}
.brand i {{ width: 2px; height: 40px; background: #121a2c; }}
.chev {{ width: 18px; height: 22px; background: #e5383b; clip-path: polygon(0 0, 55% 0, 100% 50%, 55% 100%, 0 100%, 45% 50%); }}
h1 {{ position: absolute; left: 38px; top: 112px; font-size: 92px; font-weight: 700; letter-spacing: -2px; }}
h1 span {{ color: #e5383b; }}
.rule {{ position: absolute; left: 42px; top: 232px; width: 470px; height: 4px; border-radius: 2px;
  background: linear-gradient(90deg, #2f6fe4, #9b5de5, #e5383b, #f59e5b); }}
.tag {{ position: absolute; left: 42px; top: 256px; font-size: 32px; font-weight: 700; line-height: 1.3; }}
.feats {{ position: absolute; left: 50px; top: 362px; display: flex; gap: 22px; }}
.feat {{ display: flex; align-items: center; gap: 10px; font-size: 16px; line-height: 1.25; }}
.butler {{ position: absolute; left: 34px; top: 440px; width: 180px; height: 180px; border-radius: 50%;
  background: #d33833; overflow: hidden; }}
.butler img {{ position: absolute; left: 22px; top: 14px; width: 150px; }}
.win {{ position: absolute; border-radius: 12px; box-shadow: 0 10px 30px rgba(30,45,90,.18); overflow: hidden; }}
.editor {{ left: 560px; top: 34px; width: 420px; height: 262px; background: #fff; border: 1px solid #d8deea; }}
.editor .tab {{ height: 38px; background: #f3f5fa; border-bottom: 1px solid #e3e7f0; display: flex; align-items: center;
  padding-left: 60px; font-size: 15px; font-weight: 700; }}
.editor .side {{ position: absolute; left: 0; top: 0; bottom: 0; width: 44px; background: #1c2333; }}
.editor .body {{ position: absolute; left: 44px; top: 46px; right: 0; font: 14px/29px Mono; }}
.ln {{ display: flex; white-space: pre; padding-right: 8px; }}
.ln .num {{ width: 34px; text-align: right; padding-right: 12px; color: #9aa3b5; }}
.ln.bad {{ background: #fde8e8; }}
.ln.bad .num {{ color: #e5383b; font-weight: 700; }}
.term {{ left: 560px; top: 318px; width: 440px; height: 196px; background: #0f1420; color: #e6edf3; }}
.term .bar {{ height: 30px; background: #181e2c; display: flex; align-items: center; padding-left: 12px; gap: 7px;
  font: 700 13px Outfit; color: #c8d0e0; }}
.term .bar i {{ width: 11px; height: 11px; border-radius: 50%; }}
.term .bar b {{ margin-left: 120px; }}
.term pre {{ font: 11.4px/17px Mono; padding: 9px 14px; }}
.dim {{ color: #7d8590; }} .red {{ color: #f85149; }} .blue {{ color: #58a6ff; }} .purple {{ color: #bc8cff; }}
.card {{ position: absolute; left: 1024px; top: 188px; width: 220px; height: 260px; background: #fff; border-radius: 14px;
  border: 1px solid #d8deea; box-shadow: 0 10px 30px rgba(30,45,90,.14); text-align: center; padding-top: 18px; }}
.card .head {{ display: flex; align-items: center; justify-content: center; gap: 8px; font-size: 24px; font-weight: 700; }}
.card .head img {{ width: 30px; }}
.card .opt {{ display: inline-block; margin: 26px 0 14px; font: 700 15px Mono; color: #2f6fe4; background: #eaf0fd;
  padding: 8px 14px; border-radius: 8px; }}
.card p {{ font-size: 16px; line-height: 1.3; }}
.link {{ position: absolute; border-top: 2px dashed #2f6fe4; }}
.flow {{ position: absolute; left: 300px; top: 536px; display: flex; align-items: center; gap: 0; }}
.step {{ display: flex; align-items: center; gap: 12px; background: #fff; border: 1px solid #d8deea; border-radius: 12px;
  padding: 14px 18px; font-size: 15px; line-height: 1.25; box-shadow: 0 6px 16px rgba(30,45,90,.08); }}
.arrow {{ width: 44px; height: 2px; background: #2f6fe4; position: relative; }}
.arrow.dash {{ background: none; border-top: 2px dashed #2f6fe4; }}
.arrow::after {{ content: ""; position: absolute; right: -2px; top: -6px; border: 6px solid transparent;
  border-left: 8px solid #2f6fe4; border-right: 0; }}
.arrow.dash::after {{ top: -8px; }}
</style>
<div class=grid></div>
<div class=brand><div class=chev></div><i></i>jenkins-lint</div>
<h1>jenkins-<span>lint</span></h1>
<div class=rule></div>
<div class=tag>Catch Jenkinsfile mistakes<br>before they reach CI</div>
<div class=feats>{features}</div>
<div class=butler><img src="{LOGO}"></div>
<div class="win editor"><div class=side></div><div class=tab>Jenkinsfile</div><div class=body>{code}</div></div>
<div class="win term"><div class=bar><i style="background:#ff5f57"></i><i style="background:#febc2e"></i>
<i style="background:#28c840"></i><b>jenkins-lint</b></div><pre>{TERMINAL}</pre></div>
<div class="link" style="left:1000px; top:416px; width:24px"></div>
<div class=card><div class=head><img src="{LOGO}">Jenkins</div><div class=opt>--remote</div>
<p>Also validate with<br>your own Jenkins</p></div>
<div class=flow>{flow}</div>
"""

with tempfile.TemporaryDirectory() as tmp:
    page = Path(tmp) / "social.html"
    page.write_text(HTML, encoding="utf-8")
    subprocess.run(
        [
            CHROME,
            "--headless",
            "--disable-gpu",
            "--hide-scrollbars",
            "--allow-file-access-from-files",
            "--force-device-scale-factor=1",
            "--window-size=1280,640",
            f"--screenshot={OUT}",
            page.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
print(OUT)
