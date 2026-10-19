#!/usr/bin/env python3
"""Compare the offline linter with the official Declarative validator.

The pipeline-model-definition plugin ships hundreds of test Jenkinsfiles:
valid ones, and ones under errors/ whose expected messages are listed in
ValidatorTest.java. This script runs the offline linter over all of them
and reports how often it agrees with the official validator.

Usage:
    python3 crates/jenkinsfilelint/scripts/corpus_check.py [--plugin-dir DIR] [--binary PATH] [-v]

Without --plugin-dir the plugin is cloned (sparse, pinned commit) into
crates/jenkinsfilelint/target/corpus.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

PLUGIN_REPO = "https://github.com/jenkinsci/pipeline-model-definition-plugin"
PLUGIN_COMMIT = "6e7193cec599b402ee79ee9afc5b8a417eb43967"
BASE = "pipeline-model-definition"
RESOURCES = f"{BASE}/src/test/resources"
VALIDATOR_TEST = f"{BASE}/src/test/java/org/jenkinsci/plugins/pipeline/modeldefinition/ValidatorTest.java"
MESSAGES = f"{BASE}/src/main/resources/org/jenkinsci/plugins/pipeline/modeldefinition"

# Files outside errors/ that the official tests expect to fail validation,
# or whose tests are @Ignore'd because they no longer validate.
KNOWN_INVALID = {
    "when/whenEmpty.groovy": "WhenStageTest expects ModelValidatorImpl.EmptyWhen",
    "when/conditions/allOfEmpty.groovy": "WhenStageTest expects NestedWhenWithoutChildren",
    "matrix/matrixStagesGroupsAndStages.groovy": "@Ignore'd in MatrixTest; has no axes",
    "matrix/matrixStagesHaveStatusAndPost.groovy": "@Ignore'd in MatrixTest; has no axes",
    "matrix/matrixStagesNestedInSequential.groovy": "@Ignore'd in MatrixTest; has no axes",
    "matrix/matrixStagesShouldntTriggerNSE.groovy": "@Ignore'd in MatrixTest; has no axes",
}

# Errors the official validator can only find with plugins or a running
# controller (step signatures, tool installations, script security, etc.).
NEEDS_CONTROLLER = re.compile(
    r"InvalidStepParameter|MissingRequiredStepParameter|WrongNumberOfStepParameters|InvalidParameterType|"
    r"InvalidUnnamedParameterType|NoToolVersion|InvalidSectionType|RejectedAccessException|"
    r"MultipleCompilationErrorsException|TooManyUnnamedParameters|MixedNamedAndUnnamedParameters|"
    r"WrongBuildParameterType|NotSingleRequiredParameter|InvalidAgentParameter|MissingAgentParameter|"
    r"MultipleAgentParameters|InvalidAgentType"
)


def fetch_plugin(dest: Path) -> Path:
    if (dest / RESOURCES).is_dir():
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)

    def run(*cmd: str) -> None:
        subprocess.run(cmd, check=True, cwd=dest)

    subprocess.run(
        ["git", "clone", "--filter=blob:none", "--no-checkout", PLUGIN_REPO, str(dest)],
        check=True,
    )
    run("git", "sparse-checkout", "set", RESOURCES, VALIDATOR_TEST, MESSAGES)
    run("git", "checkout", PLUGIN_COMMIT)
    return dest


def load_messages(root: Path) -> dict:
    out = {}
    lines = []
    for path in sorted(root.rglob("Messages.properties")):
        lines += path.read_text(encoding="utf-8").splitlines()
    for line in lines:
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            v = (
                v.encode("utf-8")
                .decode("unicode_escape")
                .replace("'{'", "{")
                .replace("\\'", "'")
                .replace("''", "'")
            )
            out[k.strip().replace(".", "_")] = v.strip()
    return out


def template_regex(template: str) -> re.Pattern:
    parts = re.split(r"\{\d+\}", template)
    return re.compile(".*".join(re.escape(p) for p in parts))


def expected_errors(test: Path, messages: dict) -> dict:
    """Map errors/<name>.groovy to the message keys ValidatorTest expects."""
    src = test.read_text(encoding="utf-8")
    out = {}
    for m in re.finditer(r'expectError\("(\w+)"\)(.*?);', src, re.DOTALL):
        keys = re.findall(r"Messages\.(\w+)\(", m.group(2))
        literals = re.findall(r'logContains\("([^"]*)"', m.group(2))
        out[m.group(1)] = {"keys": keys, "literals": literals}
    return out


def lint(binary: str, path: Path) -> list:
    r = subprocess.run(
        [binary, "--format", "json", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode == 2:
        raise SystemExit(r.stderr)
    return json.loads(r.stdout or "[]")


def main() -> int:
    here = Path(__file__).resolve().parent.parent
    ap = argparse.ArgumentParser()
    ap.add_argument("--plugin-dir", type=Path)
    ap.add_argument("--binary", default=str(here / "target/release/jenkinsfilelint"))
    ap.add_argument("-v", "--verbose", action="store_true")
    args = ap.parse_args()

    plugin = args.plugin_dir or fetch_plugin(
        here / "target/corpus/pipeline-model-definition-plugin"
    )
    resources = plugin / RESOURCES
    messages = load_messages(plugin / MESSAGES)
    expected = expected_errors(plugin / VALIDATOR_TEST, messages)

    # Valid files: no errors expected.
    valid = [
        p
        for p in sorted(resources.rglob("*.groovy"))
        if "errors" not in p.relative_to(resources).parts
    ]
    valid = [p for p in valid if str(p.relative_to(resources)) not in KNOWN_INVALID]
    false_pos = []
    for p in valid:
        errs = [d for d in lint(args.binary, p) if d["severity"] == "error"]
        if errs:
            false_pos.append((p.relative_to(resources), errs[0]))

    # Error files with a ValidatorTest expectation.
    caught, same_msg, missed, controller_only = [], [], [], []
    for name, exp in sorted(expected.items()):
        path = resources / "errors" / f"{name}.groovy"
        if not path.exists():
            continue
        if any(NEEDS_CONTROLLER.search(k) for k in exp["keys"]) or (
            exp["literals"] and not exp["keys"]
        ):
            flagged = any(d["severity"] == "error" for d in lint(args.binary, path))
            controller_only.append((name, flagged))
            continue
        got = [
            d["message"] for d in lint(args.binary, path) if d["severity"] == "error"
        ]
        if not got:
            missed.append(name)
            continue
        caught.append(name)
        patterns = [template_regex(messages[k]) for k in exp["keys"] if k in messages]
        if any(pat.search(msg) for pat in patterns for msg in got):
            same_msg.append(name)
        elif args.verbose:
            print(
                f"  different message for {name}: got {got[0]!r}, expected {exp['keys']}"
            )

    checkable = len(caught) + len(missed)
    print(f"Official validator corpus @ {PLUGIN_COMMIT[:10]}")
    print(
        f"  valid files without false errors: {len(valid) - len(false_pos)}/{len(valid)}"
    )
    print(f"  invalid files caught:             {len(caught)}/{checkable}")
    print(f"  ... with the official message:    {len(same_msg)}/{checkable}")
    print(
        f"  need plugins/controller to judge:  {len(controller_only)} "
        f"(flagged anyway: {sum(1 for _, f in controller_only if f)})"
    )
    for rel, d in false_pos:
        print(f"  FALSE ERROR {rel}:{d['line']}: {d['message']}")
    for name in missed:
        print(f"  MISSED errors/{name}.groovy expected {expected[name]['keys']}")
    return 1 if false_pos else 0


if __name__ == "__main__":
    sys.exit(main())
