#!/usr/bin/env python3
"""Negative gate test: the regression must refuse any songcore_probe whose
embedded closure identity does not exactly match the expected test-closure
manifest — BEFORE a single corpus case runs.

Cases (stubs are deterministic mismatch fixtures per the corrective's
adversarial matrix; a real wrong-closure probe produced by building the
codec-base session is exercised the same way):
  wrong closure    codec-base identity vs songcore-test expectation
  old probe        identity command unsupported (pre-corrective binary)
  garbage          identity output is not JSON
  missing field    identity record lacks profile_sha256
  positive control identity matches -> preflight PASS and the corpus starts

"0 corpus cases executed" is proven mechanically: the preflight runs before
the authority tree directory is created, so after every rejecting case the
--out directory must not exist.
As a positive control, a matching stub lets the corpus start (its records
fail the semantic gates — that is expected and outside this gate).
Everything lives under build/ (regenerable).
"""
import json
import os
import stat
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(ROOT, "tests", "songcore"))
import regression  # noqa: E402

SCRATCH = os.path.join(ROOT, "build", "identity-gate-test")
EXPECTED_MANIFEST = regression.DEFAULT_EXPECTED_MANIFEST


def make_stub(name, lines=(), exit_code=0):
    path = os.path.join(SCRATCH, name)
    body = ["#!/usr/bin/env python3", "import sys"]
    for line in lines:
        body.append(f"sys.stdout.write({line!r})")
    body.append(f"sys.stderr.write('stub probe: identity refused\\n')")
    body.append(f"sys.exit({exit_code})")
    with open(path, "w") as f:
        f.write("\n".join(body) + "\n")
    os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC)
    return path


def stub_identity(expected, overrides=None, omit=()):
    rec = dict(expected)
    rec["kind"] = "songcore-probe-identity"
    rec.update(overrides or {})
    for k in omit:
        del rec[k]
    return json.dumps(rec) + "\n"


def run_regression(binary, out_dir):
    return subprocess.run(
        [sys.executable, os.path.join(ROOT, "tests", "songcore",
                                      "regression.py"),
         "--binary", binary, "--out", out_dir, "--no-common"],
        capture_output=True, text=True, cwd=ROOT)


def require_rejected(label, binary):
    out_dir = os.path.join(SCRATCH, "out-" + label)
    p = run_regression(binary, out_dir)
    combined = p.stdout + p.stderr
    if p.returncode == 0:
        raise SystemExit(f"GATE BROKEN: {label} ran the regression to PASS")
    if p.returncode != 2:
        raise SystemExit(f"GATE BROKEN: {label} failed the wrong way "
                         f"(exit {p.returncode}):\n{combined[-2000:]}")
    if "FATAL" not in combined:
        raise SystemExit(f"GATE BROKEN: {label} failed without the identity "
                         f"FATAL report:\n{combined[-2000:]}")
    if "has NOT been executed" not in combined:
        raise SystemExit(f"GATE BROKEN: {label} did not state the corpus "
                         f"was not executed:\n{combined[-2000:]}")
    if os.path.exists(out_dir):
        raise SystemExit(f"GATE BROKEN: {label} executed corpus cases "
                         f"(authority tree was written): {out_dir}")
    for field in ("expected profile:", "actual   profile:"):
        if field not in p.stdout:
            raise SystemExit(f"GATE BROKEN: {label} report lacks "
                             f"'{field}':\n{p.stdout[-2000:]}")
    print(f"negative gate: {label} rejected before corpus (exit 2, "
          f"no authority tree written) — PASS")


def main():
    os.makedirs(SCRATCH, exist_ok=True)
    if not os.path.isfile(EXPECTED_MANIFEST):
        raise SystemExit(f"missing expected manifest {EXPECTED_MANIFEST}; "
                         "derive the songcore-test closure first")
    expected = regression.manifest_identity(EXPECTED_MANIFEST)

    # --- negative cases: every failure mode must fail closed, pre-corpus ---
    require_rejected(
        "wrong-closure", make_stub(
            "stub_wrong_closure",
            [stub_identity(expected, {"profile": "codec-base",
                                      "profile_sha256": "0" * 64})]))
    require_rejected(
        "drifted-profile-sha", make_stub(
            "stub_drifted_sha",
            [stub_identity(expected, {"profile_sha256": "1" * 64})]))
    require_rejected(
        "old-probe-no-identity", make_stub(
            "stub_old_probe", [], exit_code=2))
    require_rejected(
        "garbage-identity", make_stub(
            "stub_garbage", ["not json at all\n"]))
    require_rejected(
        "missing-identity-field", make_stub(
            "stub_missing_field",
            [stub_identity(expected, omit=("profile_sha256",))]))

    # --- positive control: matching identity admits the corpus -------------
    out_dir = os.path.join(SCRATCH, "out-matching")
    p = run_regression(
        make_stub("stub_matching", [stub_identity(expected)]),
        out_dir)
    if "identity gate: PASS" not in p.stdout:
        raise SystemExit(f"GATE BROKEN: matching identity did not pass the "
                         f"preflight:\n{p.stdout[-2000:]}")
    if "FATAL" in p.stdout or p.returncode == 2:
        raise SystemExit(f"GATE BROKEN: matching identity was rejected:\n"
                         f"{p.stdout[-2000:]}")
    if p.returncode == 0:
        raise SystemExit("unexpected: a stub probe passed the whole corpus")
    if not os.path.isdir(out_dir):
        raise SystemExit("positive control never started the corpus")
    print("positive control: matching identity passed the preflight and the "
          "corpus started (stub records fail semantic gates as expected) "
          "— PASS")

    print("probe closure identity gate: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
