#!/usr/bin/env python3

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG_PATH = ROOT / ".github" / "commit-convention.json"


def load_config() -> dict:
    with CONFIG_PATH.open("r", encoding="utf-8") as f:
        return json.load(f)


def compile_policy(config: dict):
    types = config["types"]
    type_group = "|".join(re.escape(t) for t in types)
    scope = config["scope_pattern"]
    header = re.compile(
        rf"^(?P<type>{type_group})\((?P<scope>{scope[1:-1]})\)(?P<breaking>!)?: (?P<subject>.+)$"
    )
    forbidden_scopes = [re.compile(p) for p in config["forbidden_scope_patterns"]]
    subject_pattern = re.compile(config["subject_pattern"])
    return header, forbidden_scopes, subject_pattern


def validate_header(message: str, label: str, config: dict) -> list[str]:
    header, forbidden_scopes, subject_pattern = compile_policy(config)
    errors: list[str] = []

    if not message:
        return [f"{label}: message is empty"]

    if len(message) > config["header_max_length"]:
        errors.append(
            f"{label}: header is {len(message)} characters; maximum is {config['header_max_length']}"
        )

    match = header.fullmatch(message)
    if match is None:
        allowed = ", ".join(config["types"])
        errors.append(
            f"{label}: expected '<type>(<scope>): <subject>' or '<type>(<scope>)!: <subject>'; allowed types: {allowed}"
        )
        return errors

    scope = match.group("scope")
    subject = match.group("subject")

    for pattern in forbidden_scopes:
        if pattern.fullmatch(scope):
            errors.append(
                f"{label}: scope '{scope}' describes project-management staging rather than a responsibility domain"
            )
            break

    if subject_pattern.match(subject) is None:
        errors.append(
            f"{label}: subject must begin with a lowercase ASCII letter or digit"
        )

    for suffix in config["forbidden_subject_suffixes"]:
        if subject.endswith(suffix):
            errors.append(f"{label}: subject must not end with '{suffix}'")
            break

    return errors


def check_message(message: str, label: str, config: dict) -> int:
    errors = validate_header(message, label, config)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"       got: {message}", file=sys.stderr)
        return 1
    print(f"PASS: {label}: {message}")
    return 0


def check_records_file(path: Path, config: dict) -> int:
    failed = False
    with path.open("r", encoding="utf-8") as f:
        for line_no, raw in enumerate(f, 1):
            line = raw.rstrip("\n")
            if not line:
                continue
            if "\t" not in line:
                print(
                    f"ERROR: {path}:{line_no}: expected '<label>\\t<message>'",
                    file=sys.stderr,
                )
                failed = True
                continue
            label, message = line.split("\t", 1)
            failed = bool(check_message(message, f"commit {label}", config)) or failed
    return 1 if failed else 0


def git_subjects(base: str, head: str) -> list[tuple[str, str]]:
    revs = subprocess.run(
        ["git", "rev-list", "--reverse", f"{base}..{head}"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    result: list[tuple[str, str]] = []
    for sha in revs:
        subject = subprocess.run(
            ["git", "show", "-s", "--format=%s", sha],
            check=True,
            text=True,
            capture_output=True,
        ).stdout.rstrip("\n")
        result.append((sha, subject))
    return result


def check_range(base: str, head: str, config: dict) -> int:
    failed = False
    subjects = git_subjects(base, head)
    if not subjects:
        print(f"ERROR: no commits found in range {base}..{head}", file=sys.stderr)
        return 1
    for sha, subject in subjects:
        failed = bool(check_message(subject, f"commit {sha[:12]}", config)) or failed
    return 1 if failed else 0


def self_test(config: dict) -> int:
    valid = [
        "feat(plugin): add SongCore decode provider",
        "fix(native): preserve SongSource callback lifetime",
        "refactor(runtime): remove hardcoded output registration",
        "perf(ffi): record SongCore call boundary cost",
        "test(kernel): prove provider withdrawal ordering",
        "docs(perf): record decode cost model",
        "build(native): link SongCore static closure",
        "ci(review): enable draft PR review",
        "chore(repo): define commit convention policy",
        "revert(runtime): restore previous composition behavior",
        "feat(plugin)!: replace decoder capability contract",
    ]
    invalid = [
        "ci feat: add decoder",
        "feat ci: add decoder",
        "chore/fix(native): fix decoder",
        "test+fix(native): fix decoder",
        "foo(native): add decoder",
        "Feat(native): add decoder",
        "feat: add decoder",
        "feat(E2): add decoder",
        "feat(phase-e): add decoder",
        "feat(round-2): add decoder",
        "docs(native): Add decoder docs",
        "docs(native): add decoder docs.",
    ]

    failures: list[str] = []
    for message in valid:
        if validate_header(message, "self-test", config):
            failures.append(f"valid message rejected: {message}")
    for message in invalid:
        if not validate_header(message, "self-test", config):
            failures.append(f"invalid message accepted: {message}")

    if failures:
        for failure in failures:
            print(f"ERROR: {failure}", file=sys.stderr)
        return 1
    print(f"PASS: self-test ({len(valid)} valid, {len(invalid)} invalid cases)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--message")
    mode.add_argument("--records-file", type=Path)
    mode.add_argument("--range", nargs=2, metavar=("BASE", "HEAD"))
    mode.add_argument("--self-test", action="store_true")
    parser.add_argument("--label", default="message")
    args = parser.parse_args()

    config = load_config()

    if args.message is not None:
        return check_message(args.message, args.label, config)
    if args.records_file is not None:
        return check_records_file(args.records_file, config)
    if args.range is not None:
        return check_range(args.range[0], args.range[1], config)
    return self_test(config)


if __name__ == "__main__":
    raise SystemExit(main())
