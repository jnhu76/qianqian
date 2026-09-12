#!/usr/bin/env python3
"""Architecture vocabulary drift gate.

Scans current production code, active automation, and derived documentation
for high-confidence stale architecture identifiers retired by ADR-PBK-002.
Historical evidence is intentionally excluded when rewriting it would falsify
provenance.

Exit 0 = PASS, Exit 1 = FAIL.
Uses only the Python standard library.
"""

from __future__ import annotations

import os
import re
import sys
import tempfile
from pathlib import Path

STALE_CARGO_TOML = [
    (r'^name\s*=\s*"qianqian-kernel"', "old package name qianqian-kernel"),
    (r'^name\s*=\s*"qianqian-core"', "old package name qianqian-core"),
    (r'^name\s*=\s*"qianqian-runtime"', "old package name qianqian-runtime"),
]

STALE_RS_IMPORTS = [
    (r"\buse\s+qianqian_kernel\b", "old import path qianqian_kernel"),
    (r"\buse\s+qianqian_core\b", "old import path qianqian_core"),
    (r"\buse\s+qianqian_runtime\b", "old import path qianqian_runtime"),
]

STALE_RS_KERNEL_TYPE = [
    (r"(?<!Composition)(?<!Music)(?<!Transport)(?<!\")\bKernel::", "standalone Kernel:: call"),
    (r"= Kernel::new", "= Kernel::new constructor"),
    (r": Kernel[,\s>]", ": Kernel type annotation"),
    (r"-> Kernel[,\s>]", "-> Kernel return type"),
    (r"\(Kernel\)", "(Kernel) type"),
    (r"\bKernel,\s*Revision", "Kernel in import list"),
    (r"\bKernel,\s*StepOutcome", "Kernel in import list"),
    (r"\bAppRuntime\b", "AppRuntime type"),
]

STALE_TOML_DEPS = [
    (r"qianqian-kernel\s*=", "old dependency qianqian-kernel"),
    (r"qianqian-core\s*=", "old dependency qianqian-core"),
    (r"qianqian-runtime\s*=", "old dependency qianqian-runtime"),
]

STALE_TEXT_REFERENCES = [
    (r"crates/qianqian-kernel", "stale crate path"),
    (r"crates/qianqian-core", "stale crate path"),
    (r"crates/qianqian-runtime", "stale crate path"),
    (r"qianqian_kernel::", "stale crate import in doc/script"),
    (r"qianqian_core::", "stale crate import in doc/script"),
    (r"qianqian_runtime::", "stale crate import in doc/script"),
    (r"-p qianqian-kernel", "stale cargo -p reference"),
    (r"-p qianqian-core", "stale cargo -p reference"),
    (r"-p qianqian-runtime", "stale cargo -p reference"),
    (r"qianqian-kernel\s*=", "stale dependency spec"),
    (r"qianqian-core\s*=", "stale dependency spec"),
    (r"qianqian-runtime\s*=", "stale dependency spec"),
    (r"name\s*=\s*\"qianqian-kernel\"", "stale package name"),
    (r"name\s*=\s*\"qianqian-core\"", "stale package name"),
    (r"name\s*=\s*\"qianqian-runtime\"", "stale package name"),
]

EXCLUDE_DIRS = {
    ".git",
    "target",
    "node_modules",
    ".zcode",
    "dist",
}

# Narrow historical exclusions only. Active .github workflows are deliberately
# scanned because CI/build automation is part of the current execution surface.
EXCLUDE_PATH_SUBSTRINGS = [
    "docs/archive/",
    "playback_temporal_traces/",
    "evidence/",
]

EXCLUDE_FILES = {
    "ADR-PBK-001.md",
    "history.md",
    "check_architecture_vocabulary.py",
}

SCANNABLE_EXTENSIONS = {".rs", ".toml", ".md", ".yml", ".yaml", ".py", ".sh", ".ts", ".js"}


def should_exclude(filepath: str) -> bool:
    parts = filepath.split(os.sep)
    if any(directory in parts for directory in EXCLUDE_DIRS):
        return True
    if any(fragment in filepath for fragment in EXCLUDE_PATH_SUBSTRINGS):
        return True
    return os.path.basename(filepath) in EXCLUDE_FILES


def scan_file(filepath: str, stale_patterns: list[tuple[str, str]]):
    violations = []
    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as handle:
            for lineno, line in enumerate(handle, 1):
                for pattern, description in stale_patterns:
                    if re.search(pattern, line):
                        violations.append((filepath, lineno, description, line.rstrip()))
    except OSError:
        pass
    return violations


def find_files(root: str):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRS]
        for filename in filenames:
            if Path(filename).suffix in SCANNABLE_EXTENSIONS:
                yield os.path.join(dirpath, filename)


def patterns_for(filepath: str):
    ext = Path(filepath).suffix
    if ext == ".toml":
        return STALE_CARGO_TOML + STALE_TOML_DEPS
    if ext == ".rs":
        return STALE_RS_IMPORTS + STALE_RS_KERNEL_TYPE
    return STALE_TEXT_REFERENCES


def collect_violations(root: str):
    violations = []
    for filepath in find_files(root):
        if should_exclude(filepath):
            continue
        violations.extend(scan_file(filepath, patterns_for(filepath)))
    return violations


def run_negative_control() -> None:
    """Prove an active workflow stale-name mutation is caught, then clears."""
    with tempfile.TemporaryDirectory() as tmp:
        workflow_dir = Path(tmp) / ".github" / "workflows"
        workflow_dir.mkdir(parents=True)
        workflow = workflow_dir / "negative-control.yml"

        if should_exclude(str(workflow)):
            raise AssertionError("active .github workflow must not be excluded")

        workflow.write_text(
            "name: negative-control\njobs:\n  test:\n    steps:\n      - run: cargo test -p qianqian-kernel\n",
            encoding="utf-8",
        )
        bad = collect_violations(tmp)
        if not any("qianqian-kernel" in line for _, _, _, line in bad):
            raise AssertionError("negative control was not detected")

        workflow.write_text(
            "name: negative-control\njobs:\n  test:\n    steps:\n      - run: cargo test -p qianqian-composition\n",
            encoding="utf-8",
        )
        good = collect_violations(tmp)
        if good:
            raise AssertionError(f"corrected negative control still fails: {good}")


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    try:
        run_negative_control()
    except AssertionError as exc:
        print(f"VOCABULARY GATE SELF-TEST FAIL: {exc}")
        return 1

    violations = collect_violations(root)
    if violations:
        print(f"VOCABULARY GATE FAIL: {len(violations)} stale identifier(s) found\n")
        for filepath, lineno, description, line in violations:
            relpath = os.path.relpath(filepath, root)
            print(f"  {relpath}:{lineno}: [{description}]")
            print(f"    {line}")
        print("\nSee ADR-PBK-002 for canonical vocabulary.")
        return 1

    print("VOCABULARY GATE PASS: no stale identifiers found; workflow negative control FAIL→PASS verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
