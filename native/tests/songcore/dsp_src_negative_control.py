#!/usr/bin/env python3
"""NC-DSP-STALE-1: oracle-sensitivity control for dsp_src.py::run_probe.

Proves the DSP/SRC gate cannot reproduce the BUG-1 vacuous PASS, where a
failed probe (exit non-zero, no output) was silently papered over by a
stale fixed-name /tmp/dsp-src.json.

NC-A (historical failure shape): a valid stale PASS JSON sits at the gate's
reserved output path and the probe exits non-zero writing nothing.
Expected: gate FAILs on the returncode check; the stale JSON is never
parsed. Pre-corrective this exact shape produced a FALSE PASS.

NC-B (silent-success shape): the probe exits 0 but writes no output.
Expected: gate FAILs on the freshly-produced-output check.

Exit 0 iff both controls hold. Test-only, never ships; writes nothing
outside its own temporary directory and never touches the committed
authority tree.
"""

import json
import os
import stat
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import dsp_src  # noqa: E402

AUTHORITY = os.path.join(HERE, "..", "..", "..", "bench", "results",
                         "songcore-v1", "dsp-src-integration.json")

FAIL_STUB = "#!/bin/sh\necho 'NC stub stdout: partial output'\n" \
            "echo 'NC stub stderr: intentional probe failure' >&2\nexit 3\n"
SILENT_STUB = "#!/bin/sh\nexit 0\n"


def write_stub(workdir, name, content):
    path = os.path.join(workdir, name)
    with open(path, "w") as f:
        f.write(content)
    os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR | stat.S_IXGRP
             | stat.S_IXOTH)
    return path


def expect_gate_failure(label, stub, reserve):
    orig_probe, orig_reserve = dsp_src.PROBE, dsp_src._reserve_output_path
    dsp_src.PROBE = stub
    dsp_src._reserve_output_path = reserve
    try:
        dsp_src.run_probe()
    except SystemExit as e:
        msg = str(e)
        if label == "NC-A" and "rc=3" not in msg:
            print(f"{label} FAIL: exit message lacks probe returncode:\n{msg}")
            return None
        if label == "NC-A" and "NC stub stderr" not in msg:
            print(f"{label} FAIL: probe stderr not surfaced:\n{msg}")
            return None
        if label == "NC-B" and "freshly" not in msg and "no output" not in msg:
            print(f"{label} FAIL: wrong failure mode:\n{msg}")
            return None
        return msg
    finally:
        dsp_src.PROBE, dsp_src._reserve_output_path = orig_probe, orig_reserve
    print(f"{label} FAIL: run_probe returned data — the gate would have "
          f"accepted it (false PASS not excluded)")
    return None


def main():
    failures = []
    workdir = tempfile.mkdtemp(prefix="nc-dsp-stale-")
    try:
        # Valid stale PASS JSON: a copy of the committed authority, i.e.
        # exactly the payload a false PASS would need.
        stale = os.path.join(workdir, "stale-pass.json")
        with open(AUTHORITY) as src, open(stale, "w") as dst:
            dst.write(src.read())
        with open(stale) as f:
            d = json.load(f)
        if d.get("verdict") != "PASS" or not all(
                g.get("ok") for g in d.get("gates", [])):
            failures.append("stale fixture is not a valid PASS JSON")
            return finish(failures)

        # NC-A: failing probe + stale JSON at the output path.
        stub = write_stub(workdir, "failing-probe.sh", FAIL_STUB)
        msg = expect_gate_failure("NC-A", stub, lambda: stale)
        if msg is None:
            failures.append("NC-A: failed probe did not fail the gate")
        else:
            # The gate's finally-cleanup may remove the stale payload; that
            # is destruction, not consumption — the forbidden behavior
            # (parsing it into a PASS) is excluded by the SystemExit above.
            print("NC-A PASS: failed probe (rc=3) fails the gate; stale "
                  "PASS JSON never parsed")

        # NC-B: exit-0 probe that writes nothing, genuinely fresh path.
        fresh = os.path.join(workdir, "fresh.json")
        with open(fresh, "w"):
            pass
        stub = write_stub(workdir, "silent-probe.sh", SILENT_STUB)
        msg = expect_gate_failure("NC-B", stub, lambda: fresh)
        if msg is None:
            failures.append("NC-B: silent probe did not fail the gate")
        else:
            print("NC-B PASS: exit-0 probe without fresh output fails the "
                  "gate")
    finally:
        for name in os.listdir(workdir):
            os.unlink(os.path.join(workdir, name))
        os.rmdir(workdir)
    return finish(failures)


def finish(failures):
    if failures:
        print("NC-DSP-STALE-1 FAIL")
        for x in failures:
            print("  -", x)
        return 1
    print("NC-DSP-STALE-1 PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
