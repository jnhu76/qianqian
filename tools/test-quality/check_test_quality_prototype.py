#!/usr/bin/env python3
"""check_test_quality_prototype.py — R1-B rev2 calibration prototype. NOT the production checker.

Purpose: turn the R1-B "design-level" claims ("H1 fires on Case 1/5, zero
signals on Case 4, H4 catches R0B-02/R0B-05 weakly") into EXECUTED results.
Prototype-grade on purpose: regex + brace walking, no Rust AST. Every
detector reports its known limits inline.

Modes:
  scan FILE...                 HEURISTIC signal scan (H1-H4) + structural notes. Exit 0 always.
  record FILE...               TER structural validation (P2a/P3/P4a). Exit 1 on structural violation.
  oracle-diff BEFORE AFTER     H5 oracle-weakening shape scan on a before/after pair. Exit 0 always.
  self-test                    run the embedded mini-fixtures (checker's own negative controls).
  calibrate CASES_DIR          run the full R1-B calibration matrix, print the table.

REV2 classification (owner-corrected):
  - H1-H4 output REVIEW_SIGNAL only. A signal NEVER means "test is wrong".
  - Record checks are STRUCTURAL ONLY: "field present / path exists / values
    in whitelist". A PASS proves the filing is complete and internally
    consistent; it carries ZERO meaning about test adequacy.
  - M/T correctness (is this test really M3? is T right?) is DOMAIN REVIEW.
"""

from __future__ import annotations
import re
import sys
from pathlib import Path

MECHANISM_KEYWORDS = re.compile(
    r"thread::spawn|Condvar|mpsc::|\.join\(\)|Atomic[A-Z]|thread::sleep"
)
CLAIM_PATTERNS = [
    (re.compile(r"post-EOF|after\s+EOF", re.I), "eof"),
    (re.compile(r"during\s+(an\s+)?(accepted\s+)?transition", re.I), "transition"),
    (re.compile(r"post-?seal", re.I), "seal"),
    (re.compile(r"while\s+parked", re.I), "parked"),
]
WITNESS_LINE = re.compile(r"wait_until|assert|recv|\.wait|observe|wait_terminal")
SLEEP_CALL = re.compile(r"\bsleep\s*\(|thread::sleep")
RESULT_WHITELIST = {
    "PASS_IN_SCOPE", "CONFIRMED_FAIL", "ORACLE_DEFECT",
    "INCONCLUSIVE", "TOOLING_FAIL", "NOT_RUN",
}
M_VALUES = {"M0", "M1", "M2", "M3", "M4"}
R_VALUES = {"R-L", "R-M", "R-H"}
T_VALUES = {"T0", "T1", "T2", "T3"}
M3_REQUIRED_FIELDS = [
    "PRECONDITION_WITNESS", "LEGAL_OUTCOMES", "OBSERVATION_BOUNDARY",
    "FALSIFIER", "EXECUTION_PROFILE", "ACTUAL_TEST_COUNT",
    "CLEANUP_GUARANTEE", "EVIDENCE_LIMITS",
]
M4_REQUIRED_FIELDS = ["PLATFORM_DECLARATION"]


def extract_fns(text: str):
    """Yield (name, base_line, body, doc) for #[test]-attributed fns. Prototype brace walker."""
    fns = []
    attr = re.compile(r"#\[(?:tokio::)?test\]")
    fnhdr = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)")
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        if attr.search(lines[i]):
            doc_start = i
            while doc_start > 0 and lines[doc_start - 1].strip().startswith("///"):
                doc_start -= 1
            doc = "\n".join(lines[doc_start:i])
            j = i
            while j < len(lines) and not fnhdr.search(lines[j]):
                j += 1
                if j - i > 4:
                    break
            if j < len(lines):
                m = fnhdr.search(lines[j])
                name = m.group(1)
                depth = 0
                started = False
                body_lines = []
                k = j
                for k in range(j, len(lines)):
                    depth += lines[k].count("{") - lines[k].count("}")
                    if "{" in lines[k]:
                        started = True
                    if started:
                        body_lines.append((k + 1, lines[k]))
                        if depth <= 0:
                            break
                fns.append((name, j + 1, body_lines, doc))
                i = k
        i += 1
    return fns


def sleep_shape(body_lines, sleep_idx):
    """Classify a sleep as 'poll' (lexically inside a loop block) or 'bare'.

    Uses loop-header brace matching, not a line window: a poll loop's
    condition chain can push the header >10 lines above the sleep
    (calibration finding on ports.rs:1323).
    Residual limit: shape detection is syntactic; a deliberately faked
    poll shell defeats it — semantics stay with review.
    """
    lines = [l for _, l in body_lines]
    for i, header in enumerate(lines):
        if not re.search(r"\bloop\b\s*\{|\bwhile\b.*\{", header):
            continue
        depth, started = 0, False
        for j in range(i, len(lines)):
            depth += lines[j].count("{") - lines[j].count("}")
            if "{" in lines[j]:
                started = True
            if started and depth <= 0:
                if i <= sleep_idx <= j:
                    return "poll"
                break
    return "bare"


def spawn_join_audit(body: str):
    """Return H3 signals: bound handles never joined; unbound spawns that do not
    join anything inside their own closure (join-delegation watchers are exempt —
    the #242 join-watcher idiom). Crude but calibrated; review adjudicates.

    Known FP class (calibrated, documented): the exit-witness idiom where the
    leg spawn is unbound and its handle is MOVED into a separate watcher closure
    that joins — the prototype only inspects the spawn's own closure, so it
    flags the leg. Review REFUTES via the suppression path.
    """
    out = []
    # precompute enclosing-block openers (stack of '{' positions)
    stack, opener_at = [], {}
    for idx, ch in enumerate(body):
        if ch == "{":
            stack.append(idx)
        elif ch == "}" and stack:
            stack.pop()
        opener_at[idx] = stack[-1] if stack else -1
    line_start_of = lambda pos: body.rfind("\n", 0, pos) + 1
    bound_handles = set(re.findall(r"(\w+)\s*=\s*(?:std::)?thread::spawn", body))
    for m in re.finditer(r"(?:std::)?thread::spawn", body):
        pre = body[max(0, m.start() - 60):m.start()]
        hm = re.search(r"(\w+)\s*=\s*$", pre)
        bound_name = hm.group(1) if hm else None
        if bound_name is None:
            # `let name = { ... spawn(...) }` block-binding idiom
            op = opener_at[m.start()]
            if op >= 0:
                line = body[line_start_of(op):body.find("\n", op)]
                bm = re.match(r"\s*(?:let\s+)?(\w+)\s*=\s*\{?\s*$", line)
                if bm and body[op:m.start()].count("thread::spawn") == 0 \
                        and body[op:m.start()].count("{") == 1:
                    bound_name = bm.group(1)
        # brace-match the spawn's own closure/argument block
        depth, started, j = 0, False, m.end()
        while j < len(body):
            if body[j] == "{":
                depth += 1
                started = True
            elif body[j] == "}":
                depth -= 1
            j += 1
            if started and depth <= 0:
                break
        closure = body[m.end():j]
        if bound_name is None:
            if not re.search(r"\.join\(|join_within", closure):
                out.append("unbound spawn whose closure never joins — detached thread risk "
                           "(or exit-witness idiom: handle moved into a joining watcher — review adjudicates)")
        else:
            bound_handles.add(bound_name)
            if not re.search(re.escape(bound_name) + r"\s*\.\s*join|join_within", body):
                out.append(f"spawned handle `{bound_name}` never joined in this fn")
    return out


def scan_file(path: Path):
    text = path.read_text()
    out = []
    for name, base, body_lines, doc in extract_fns(text):
        body = "\n".join(l for _, l in body_lines)

        # H1 — bare sleep (possible disguised-S1); poll-shape sleeps are exempt.
        for idx, (ln, line) in enumerate(body_lines):
            if SLEEP_CALL.search(line):
                shape = sleep_shape(body_lines, idx)
                tag = "tagged" if "SLEEP-PURPOSE" in body else "untagged"
                if shape == "bare":
                    out.append(("H1", f"{path}:{ln}", name,
                                f"bare sleep in test fn ({tag}); possible disguised-S1 "
                                f"premise — classify per S1-S6 (S6=PREMISE_DEBT), review must adjudicate"))

        # H2 — single-sample try_recv used as a detector.
        for ln, line in body_lines:
            if "try_recv" in line and re.search(r"assert|matches!|==", line):
                out.append(("H2", f"{path}:{ln}", name,
                            "try_recv sample asserted inline — one-sided, "
                            "timing-dependent detector (probabilistic detection)"))

        # H3 — spawn/join audit (refined: join-delegation watchers exempt).
        for msg in spawn_join_audit(body):
            out.append(("H3", f"{path}:{base}", name, msg))

        # H4 — claim word in doc comment with no witness line carrying the same root.
        for pat, root in CLAIM_PATTERNS:
            if pat.search(doc):
                witness = "\n".join(l for _, l in body_lines if WITNESS_LINE.search(l))
                if root not in witness.lower():
                    out.append(("H4", f"{path}:{base}", name,
                                f"doc claims '{root}' but no wait/assert line mentions it "
                                "(WEAK signal — token-level only)"))
    return out


# ---------------- TER structural validation (P2a/P3/P4a) ----------------

def parse_ter(text: str):
    m = re.search(r"```test-quality\s*\n(.*?)```", text, re.S)
    if not m:
        return None, "no ```test-quality block found"
    fields = {}
    for line in m.group(1).splitlines():
        fm = re.match(r"^([A-Z_]+)\s*:\s*(.*)$", line.strip())
        if fm:
            fields[fm.group(1)] = fm.group(2).strip()
    return fields, None


def validate_record(rec_path: Path, subject_file: Path | None):
    errors, notes = [], []
    fields, err = parse_ter(rec_path.read_text())
    if err:
        return [f"{rec_path}: STRUCTURAL {err}"], notes
    if not fields.get("TEST_ID") or "::" not in fields.get("TEST_ID", ""):
        errors.append(f"{rec_path}: TEST_ID missing or not crate::target::name form")
    sha = fields.get("SUBJECT_SHA", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        errors.append(f"{rec_path}: SUBJECT_SHA not 40-hex (got {sha!r})")
    if fields.get("M") not in M_VALUES:
        errors.append(f"{rec_path}: M not in {sorted(M_VALUES)}")
    if fields.get("R") not in R_VALUES:
        errors.append(f"{rec_path}: R not in {sorted(R_VALUES)}")
    if fields.get("T") not in T_VALUES:
        errors.append(f"{rec_path}: T not in {sorted(T_VALUES)}")
    if fields.get("RESULT") not in RESULT_WHITELIST:
        errors.append(f"{rec_path}: RESULT not in whitelist (got {fields.get('RESULT')!r})")

    # P3 — cited paths must exist (path part only; line numbers drift by design).
    for key in ("PRODUCTION_BOUNDARY", "AUTHORITY"):
        for ref in re.findall(r"[\w/.\-]+\.rs(?::\d+)?|docs/[\w/.\-]+\.md", fields.get(key, "")):
            p = Path(ref.split(":")[0])
            if not p.exists() and not (Path("/home/hoo/Source/qianqian-r1-flash-b") / ref.split(":")[0]).exists():
                errors.append(f"{rec_path}: {key} cites nonexistent path {ref.split(':')[0]}")
            else:
                notes.append(f"{rec_path}: {key} path resolves: {ref}")

    # Applicability-conditional structural completeness (rev2: by M, not by blanket T).
    if fields.get("M") == "M3":
        for f in M3_REQUIRED_FIELDS:
            if not fields.get(f):
                errors.append(f"{rec_path}: M3 requires {f} (mechanism-applicability, rev2)")
    if fields.get("M") == "M4":
        for f in M4_REQUIRED_FIELDS:
            if not fields.get(f):
                errors.append(f"{rec_path}: M4 requires {f}")

    # P4a — structural inconsistency between declared M and file mechanism keywords.
    # The checker proves ONLY the inconsistency; the correct M is domain review's call.
    if subject_file is not None and subject_file.exists():
        stext = subject_file.read_text()
        if MECHANISM_KEYWORDS.search(stext) and fields.get("M") in ("M0", "M1"):
            errors.append(
                f"{rec_path}: STRUCTURAL-INCONSISTENCY mechanism keywords present in "
                f"{subject_file.name} but record declares {fields.get('M')} — review must "
                "adjudicate the true M (checker does NOT assign tiers)"
            )
    return errors, notes


# ---------------- H5 oracle-weakening shape scan ----------------

def oracle_diff(before: Path, after: Path):
    out = []
    b = before.read_text().splitlines()
    a = after.read_text().splitlines()
    b_assert = [ln for ln in b if "assert" in ln]
    a_assert = [ln for ln in a if "assert" in ln]
    if len(a_assert) < len(b_assert):
        out.append(("H5a", f"assert-line count {len(b_assert)} -> {len(a_assert)}"))
    for aline, bline in zip(a_assert, b_assert):
        if "assert_eq!" in bline and "assert_eq!" not in aline and "assert" in aline:
            out.append(("H5b", f"assert_eq! -> assert!: {bline.strip()[:60]!r} -> {aline.strip()[:60]!r}"))
    for ln in a:
        m = re.search(r"Duration::from_(\w+)\((\d+)\)", ln)
        if m and "wait" in ln:
            mb = re.search(r"Duration::from_(\w+)\((\d+)\)", "\n".join(b))
            if mb and m.group(1) == mb.group(1) and int(m.group(2)) >= 10 * int(mb.group(2)):
                out.append(("H5c", f"wait budget amplified >=10x: {mb.group(2)} -> {m.group(2)}"))
    return out


# ---------------- self-test (the checker's own negative controls) ----------------

SELFTEST_DIR = Path(__file__).parent / "fixtures"


def self_test() -> int:
    fails = []
    # H1 must fire on a bare sleep and NOT on a poll-shape sleep.
    bare = SELFTEST_DIR / "h1_bare.rs"
    poll = SELFTEST_DIR / "h1_poll.rs"
    if not any(s[0] == "H1" for s in scan_file(bare)):
        fails.append("H1 did not fire on bare-sleep fixture")
    if any(s[0] == "H1" for s in scan_file(poll)):
        fails.append("H1 fired on poll-shape fixture (must be exempt)")
    # H2 must fire on inline try_recv assert.
    if not any(s[0] == "H2" for s in scan_file(SELFTEST_DIR / "h2.rs")):
        fails.append("H2 did not fire on try_recv fixture")
    # Valid M3 record must pass structurally.
    errs, _ = validate_record(SELFTEST_DIR / "ter_valid_m3.md", None)
    if errs:
        fails.append(f"valid M3 record rejected: {errs}")
    # Malformed record must fail structurally.
    errs, _ = validate_record(SELFTEST_DIR / "ter_malformed.md", None)
    if not errs:
        fails.append("malformed record accepted (must fail)")
    # Tier-lying record against a keyword-bearing file must produce STRUCTURAL-INCONSISTENCY.
    errs, _ = validate_record(SELFTEST_DIR / "ter_tier_lie.md", SELFTEST_DIR / "h1_bare.rs")
    if not any("STRUCTURAL-INCONSISTENCY" in e for e in errs):
        fails.append("tier-lie did not produce STRUCTURAL-INCONSISTENCY")
    if fails:
        for f in fails:
            print(f"SELF-TEST FAIL: {f}")
        return 1
    print("SELF-TEST OK (6/6)")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    mode = args[0]
    if mode == "self-test":
        return self_test()
    if mode == "scan":
        for f in args[1:]:
            for sig in scan_file(Path(f)):
                print(f"SIGNAL {sig[0]} {sig[1]} [{sig[2]}] {sig[3]}")
        return 0
    if mode == "record":
        rc = 0
        rec = Path(args[1])
        subj = Path(args[2]) if len(args) > 2 else None
        errs, notes = validate_record(rec, subj)
        for n in notes:
            print(f"NOTE {n}")
        for e in errs:
            print(f"STRUCTURAL-FAIL {e}")
            rc = 1
        return rc
    if mode == "oracle-diff":
        for sig in oracle_diff(Path(args[1]), Path(args[2])):
            print(f"SIGNAL {sig[0]} {sig[1]}")
        return 0
    print(f"unknown mode {mode}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
