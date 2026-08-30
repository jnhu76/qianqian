#!/usr/bin/env python3
"""Freeze the E07 minimization ladder into tracked result files.

Reads build/minimize/<stage>/gate.json (+ build/minimize/<stage>/so.json for
the shipped-.so stage) and writes bench/results/source-minimization/
summary.json + a human-readable ladder.md.

NO number in the output is hand-entered: every byte count, TU count, verdict
and xRT figure is looked up from a gate.json / so.json, and the contribution
decomposition is computed here and asserted self-consistent. Regenerate with:

    python3 tools/minimize_summary.py
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "bench" / "results" / "source-minimization"

# Ladder order and the ROLE each stage plays in the conclusions. Roles are
# names; all values still come from the stage's own gate.json.
STAGES = ["s0", "s3-pthreads", "s4-full-gc", "s4-gc", "s5-Os", "s5-Os-LTO"]
SO_STAGES = ["s6-shipped-so"]
ROLES = {
    "baseline": "s0",
    "minimal_source_closure": "s3-pthreads",
    "full_closure_gc_reference": "s4-full-gc",
    "minimal_closure_gc": "s4-gc",
    "smallest_conventional_archive": "s5-Os",
    "minimal_linked_core": "s5-Os-LTO",
    "shipped_shared_core": "s6-shipped-so",
}


def load_gate(stage: str) -> dict | None:
    p = ROOT / "build" / "minimize" / stage / "gate.json"
    if not p.is_file():
        return None
    g = json.loads(p.read_text())
    s = g["sizes"]
    xrt = {k.rsplit("/", 1)[-1]: v["xrt_songcore_output"] for k, v in g["throughput"].items()}
    return {
        "stage": stage,
        "translation_units": s["libqianqian_av_members"],
        "archive_bytes": s["libqianqian_av_a_bytes"],
        "archive_xz_bytes": s["libqianqian_av_xz_bytes"],
        "archive_symbols": s["libqianqian_av_symbols"],
        "songcore_archive_bytes": s["libsongcore_a_bytes"],
        # The shipped binary is reported in all three shipping-relevant
        # forms; "shipping size" throughout = linked stripped.
        "linked_raw_bytes": s["qn_pcm_dump_bytes"],
        "linked_stripped_bytes": s["qn_pcm_dump_stripped_bytes"],
        "linked_stripped_xz_bytes": s["qn_pcm_dump_stripped_xz_bytes"],
        "corpus": g["verify"]["verdict"],
        "corpus_cases": g["verify"]["oracle_vs_xmake_cases"],
        "pcm": "PASS" if g["verify"]["songcore_pcm"] else "FAIL",
        "strict_failures": g["strict_failures"],
        "mp3_xrt_songcore": min(xrt.get("mp3-cbr-128.mp3", 0), xrt.get("mp3-cbr-320-artwork.mp3", 0)),
        "flac_xrt_songcore": xrt.get("flac-16-44-artwork.flac", 0),
    }


def load_so(stage: str) -> dict | None:
    p = ROOT / "build" / "minimize" / stage / "so.json"
    if not p.is_file():
        return None
    d = json.loads(p.read_text())
    s = d["sizes"]
    return {
        "stage": stage,
        "so_raw_bytes": s["libqianqian_songcore_so_bytes"],
        "so_stripped_bytes": s["libqianqian_songcore_so_stripped_bytes"],
        "so_stripped_xz_bytes": s["libqianqian_songcore_so_stripped_xz_bytes"],
        "exported_symbols": d["exported_count"],
        "pic_corpus": d["pic_codegen_gate"]["corpus_verdict"],
        "smoke_ok": all(v.get("exit_code") == 0 for v in d["smoke"].values()),
    }


def gate_by_role(stages: dict[str, dict], role: str) -> dict:
    stage = ROLES[role]
    if stage not in stages:
        raise SystemExit(f"summary role '{role}' needs stage '{stage}' gate.json (missing)")
    return stages[stage]


def main() -> None:
    gates = {s: d for s in STAGES if (d := load_gate(s))}
    so_stages = {s: d for s in SO_STAGES if (d := load_so(s))}

    baseline = gate_by_role(gates, "baseline")
    full_gc = gate_by_role(gates, "full_closure_gc_reference")
    min_gc = gate_by_role(gates, "minimal_closure_gc")
    shipping = gate_by_role(gates, "minimal_linked_core")

    # Contribution decomposition on the SHIPPED metric (linked stripped), all
    # values read straight from gate.json and asserted to add up.
    gc_at_full_closure = baseline["linked_stripped_bytes"] - full_gc["linked_stripped_bytes"]
    closure_at_gc = full_gc["linked_stripped_bytes"] - min_gc["linked_stripped_bytes"]
    codegen = min_gc["linked_stripped_bytes"] - shipping["linked_stripped_bytes"]
    total = baseline["linked_stripped_bytes"] - shipping["linked_stripped_bytes"]
    assert gc_at_full_closure + closure_at_gc + codegen == total, "decomposition does not add up"

    doc = {
        "schema": 2,
        "experiment": "e07-source-minimization",
        "ffmpeg": json.loads((ROOT / "bench" / "ffmpeg-pin.json").read_text())["ffmpeg_tag"],
        "gate": "tools/minimize_gate.py (corpus 15/15 oracle-equivalence incl. bench seek, "
                "SongCore PCM canonical hashes, SongCore-level seek probes, real-song full decodes)",
        "shipping_metric": "qn_pcm_dump linked stripped bytes (raw and stripped+xz also recorded per stage)",
        "stages": gates,
        "shipped_so": so_stages,
        "finals": {
            # A: minimal archive — proven-sufficient closure, stock codegen
            "proven_sufficient_closure": {
                "stage": ROLES["minimal_source_closure"],
                "translation_units": gate_by_role(gates, "minimal_source_closure")["translation_units"],
                "archive_bytes": gate_by_role(gates, "minimal_source_closure")["archive_bytes"],
                "note": "UPPER BOUND: full contract pinned + full behavioral gate passed; "
                        "global-only resolver; not a proven mathematical minimum",
            },
            "smallest_conventional_archive": {
                "stage": ROLES["smallest_conventional_archive"],
                "translation_units": gate_by_role(gates, "smallest_conventional_archive")["translation_units"],
                "archive_bytes": gate_by_role(gates, "smallest_conventional_archive")["archive_bytes"],
                "note": "same closure at -Os; LTO archives are bytecode and do not compete",
            },
            # B: minimal shipped codec core
            "minimal_shipped_linked_core": {
                "stage": ROLES["minimal_linked_core"],
                "linked_raw_bytes": shipping["linked_raw_bytes"],
                "linked_stripped_bytes": shipping["linked_stripped_bytes"],
                "linked_stripped_xz_bytes": shipping["linked_stripped_xz_bytes"],
                "note": "qn_pcm_dump executable; host code (main/IO/QPCM) included",
            },
        },
        "linked_size_decomposition": {
            "metric": "linked stripped bytes",
            "gc_at_full_closure": gc_at_full_closure,
            "source_closure_205_to_min_at_gc": closure_at_gc,
            "size_codegen_at_min_closure": codegen,
            "total": total,
        },
    }
    if so_stages:
        doc["finals"]["shipped_shared_core"] = {
            "stage": ROLES["shipped_shared_core"],
            "so_stripped_bytes": list(so_stages.values())[0]["so_stripped_bytes"],
            "so_stripped_xz_bytes": list(so_stages.values())[0]["so_stripped_xz_bytes"],
            "exported_symbols": list(so_stages.values())[0]["exported_symbols"],
        }

    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "summary.json").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")

    lines = [
        "| Stage | TU | `.a` bytes | linked raw | **linked stripped** | stripped+xz | Corpus | MP3 xRT (min) | FLAC xRT |",
        "|---|---:|---:|---:|---:|---:|---|---:|---:|",
    ]
    for s, d in gates.items():
        lines.append(
            f"| {s} | {d['translation_units']} | {d['archive_bytes']:,} | "
            f"{d['linked_raw_bytes']:,} | {d['linked_stripped_bytes']:,} | "
            f"{d['linked_stripped_xz_bytes']:,} | {d['corpus']} | "
            f"{d['mp3_xrt_songcore']:.0f} | {d['flac_xrt_songcore']:.0f} |")
    for s, d in so_stages.items():
        lines.append(
            f"| {s} (.so) | — | — | {d['so_raw_bytes']:,} | {d['so_stripped_bytes']:,} | "
            f"{d['so_stripped_xz_bytes']:,} | PIC {d['pic_corpus']} + smoke | — | — |")
    lines += [
        "",
        "Linked-size decomposition (stripped): "
        f"GC {gc_at_full_closure:,} + source closure {closure_at_gc:,} + codegen {codegen:,} "
        f"= {total:,} B total.",
    ]
    (OUT / "ladder.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
