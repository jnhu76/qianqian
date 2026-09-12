#!/usr/bin/env python3
"""Architecture vocabulary drift gate.

Scans current production code and derived documentation for stale
architecture identifiers that were retired by ADR-PBK-002.

Usage:
    python3 tools/check_architecture_vocabulary.py

Exit 0 = PASS, Exit 1 = FAIL (stale identifiers found).

Scope:
    - .rs, .toml, .md, .yml, .yaml, .py, .sh, .ts, .js files
    - Excludes: .git/, target/, docs/archive/, historical evidence dirs

This script uses only the Python standard library.
"""

import os
import re
import sys

# ---------------------------------------------------------------------------
# Stale identifiers: high-confidence signals of vocabulary drift
# ---------------------------------------------------------------------------

# Package names (exact match in Cargo.toml or import paths)
STALE_PACKAGES = [
    "qianqian-kernel",
    "qianqian_kernel",
    "qianqian-core",
    "qianqian_core",
    "qianqian-runtime",
    "qianqian_runtime",
]

# Type identifiers (word-boundary match in Rust code)
STALE_TYPES = [
    r"\bAppRuntime\b",
    # Kernel as standalone type — but NOT CompositionKernel, MusicKernel,
    # TransportKernel, or inside comments/historical prose
    # We match: use ... Kernel, Kernel::, : Kernel, -> Kernel
    # But NOT: CompositionKernel, MusicKernel, TransportKernel
]

# Patterns that indicate stale type usage in Rust code (import/type contexts)
STALE_RUST_PATTERNS = [
    # Standalone Kernel type in use statements or type annotations
    # Match "Kernel" not preceded by "Composition" and not followed by "s"
    (r"(?<!Composition)(?<!Music)(?<!Transport)\bKernel\b(?!s)", "standalone Kernel type"),
    (r"\bAppRuntime\b", "AppRuntime type"),
]

# In Cargo.toml: old package names
STALE_CARGO_TOML = [
    (r'^name\s*=\s*"qianqian-kernel"', "old package name qianqian-kernel"),
    (r'^name\s*=\s*"qianqian-core"', "old package name qianqian-core"),
    (r'^name\s*=\s*"qianqian-runtime"', "old package name qianqian-runtime"),
]

# In .rs files: old import paths
STALE_RS_IMPORTS = [
    (r"\buse\s+qianqian_kernel\b", "old import path qianqian_kernel"),
    (r"\buse\s+qianqian_core\b", "old import path qianqian_core"),
    (r"\buse\s+qianqian_runtime\b", "old import path qianqian_runtime"),
]

# Standalone Kernel type: only match in code contexts (not prose comments)
# Match: Kernel::new, Kernel, (Kernel), : Kernel, -> Kernel
# But NOT inside doc comments (//! or ///) where "Kernel" is part of
# "Composition Kernel" prose
STALE_RS_KERNEL_TYPE = [
    (r"(?<!Composition)(?<!Music)(?<!Transport)(?<!\")\bKernel::", "standalone Kernel:: call"),
    (r"= Kernel::new", "= Kernel::new constructor"),
    (r": Kernel[,\s>]", ": Kernel type annotation"),
    (r"-> Kernel[,\s>]", "-> Kernel return type"),
    (r"\(Kernel\)", "(Kernel) type"),
    (r"\bKernel,\s*Revision", "Kernel in import list"),
    (r"\bKernel,\s*StepOutcome", "Kernel in import list"),
]

# In .toml files: old dependency references
STALE_TOML_DEPS = [
    (r"qianqian-kernel\s*=", "old dependency qianqian-kernel"),
    (r"qianqian-core\s*=", "old dependency qianqian-core"),
    (r"qianqian-runtime\s*=", "old dependency qianqian-runtime"),
]

# ---------------------------------------------------------------------------
# Exclusions: paths where historical names are allowed
# ---------------------------------------------------------------------------

EXCLUDE_DIRS = {
    ".git",
    "target",
    "node_modules",
    ".zcode",
    "dist",
}

EXCLUDE_PATH_SUBSTRINGS = [
    "docs/archive/",
    "playback_temporal_traces/",
    "evidence/",
    ".github/",
]

# Files where old names are intentional historical references
EXCLUDE_FILES = {
    "ADR-PBK-001.md",                      # historical authority, amended by PBK-002
    "history.md",                           # historical record pages
    "check_architecture_vocabulary.py",     # self (contains regex patterns for old names)
}


def should_exclude(filepath):
    """Check if this file should be excluded from scanning."""
    parts = filepath.split(os.sep)
    for d in EXCLUDE_DIRS:
        if d in parts:
            return True
    for sub in EXCLUDE_PATH_SUBSTRINGS:
        if sub in filepath:
            return True
    basename = os.path.basename(filepath)
    if basename in EXCLUDE_FILES:
        return True
    return False


def scan_file(filepath, stale_patterns):
    """Scan a single file for stale identifiers."""
    violations = []
    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for pattern, description in stale_patterns:
                    if re.search(pattern, line):
                        violations.append((filepath, lineno, description, line.rstrip()))
    except (OSError, UnicodeDecodeError):
        pass
    return violations


def find_files(root):
    """Find all scannable files under root."""
    extensions = {".rs", ".toml", ".md", ".yml", ".yaml", ".py", ".sh", ".ts", ".js"}
    for dirpath, dirnames, filenames in os.walk(root):
        # Prune excluded directories in-place
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRS]
        for fname in filenames:
            _, ext = os.path.splitext(fname)
            if ext in extensions:
                yield os.path.join(dirpath, fname)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    all_violations = []

    for filepath in find_files(root):
        if should_exclude(filepath):
            continue

        _, ext = os.path.splitext(filepath)

        if ext == ".toml":
            all_violations.extend(scan_file(filepath, STALE_CARGO_TOML + STALE_TOML_DEPS))
        elif ext == ".rs":
            all_violations.extend(scan_file(filepath, STALE_RS_IMPORTS + STALE_RS_KERNEL_TYPE))
        elif ext in {".md", ".yml", ".yaml", ".py", ".sh", ".ts", ".js"}:
            # In docs/scripts: check for stale package names in path-like contexts
            # (crates/ prefix, :: separators, Cargo.toml references, dependency specs)
            # NOT bare prose mentions (which may be intentional historical references)
            all_violations.extend(scan_file(filepath, [
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
            ]))

    if all_violations:
        print(f"VOCABULARY GATE FAIL: {len(all_violations)} stale identifier(s) found\n")
        for filepath, lineno, desc, line in all_violations:
            # Show path relative to root
            relpath = os.path.relpath(filepath, root)
            print(f"  {relpath}:{lineno}: [{desc}]")
            print(f"    {line}")
        print(f"\nSee ADR-PBK-002 for canonical vocabulary.")
        sys.exit(1)
    else:
        print("VOCABULARY GATE PASS: no stale identifiers found")
        sys.exit(0)


if __name__ == "__main__":
    main()
