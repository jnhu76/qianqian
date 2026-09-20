# ADVERSARIAL-REVIEW-1 — WINDOWS-TUI-LISTENING-RELEASE-1

Fresh-context adversarial engineering review (campaign §49), performed
by an independent reviewer agent with no prior session context, over
branch `feat/windows-tui-listening-release-1` at `12dd2fe` (evidence
tree; the product delta reviewed is `8b1a739..12dd2fe`). Reviewer
method: repository/AGENTS.md discipline read first, then reality
inspection of code, binaries (`objdump`, `strings`, `unzip`), the
cargo dependency tree for the shipped target, and the recorded
physical evidence. This document records the review's verdicts; the
review itself is evidence, not authority.

## Verdict summary

```text
Critical: 0
Required: 0
Major:    0
Minor:    3   (all fixed on this branch, see "Fixes applied")
Observations: 3 (recorded below; none in scope to change)
```

24/24 attack questions PASS. The release claims survive adversarial
inspection: the package is genuinely self-contained, the scan pipeline
is panic-free and bounded on every traced path, no architecture
boundary moved, and the documentation refuses to claim the one thing
nobody has verified yet — that it sounds right.

## Verdicts (24-point gate)

| # | Question | Verdict |
|---|----------|---------|
| 1 | Package needs Rust/Cargo/source at runtime | PASS — staging list ships exe+docs only; SongCore static (`build.rs` links `static=songcore`) |
| 2 | Runtime DLLs missing / allowlist honest | PASS — 16 imports, all Windows system DLLs inside the fail-closed allowlist; no libgcc/libstdc++/mingw runtime |
| 3 | SongCore dynamically loaded from dev path | PASS — no dlopen/LoadLibrary/DelayLoad anywhere; static link |
| 4 | qianqian-headless in ZIP | PASS — copy list stages qianqian.exe only; forbidden-content gate holds |
| 5 | Runs from arbitrary cwd | PASS — argv-only startup; physical gate ran cwd=C:\Windows\Temp with system-only PATH |
| 6/7 | Spaces/CJK path hazards | PASS — OsStr/Path handling throughout; lossy conversion display-only; physical spaces+CJK extraction evidence |
| 8 | Non-audio files quiet | PASS — `skipped` counter, no per-file diagnostic; pinned by test |
| 9 | Corrupt candidate panics scan | PASS — probe returns Result; Err → bounded rejection; no unwrap on the path |
| 10 | Unreadable dir panics | PASS — bounded diagnostic arms, test-pinned |
| 11 | Duplicate roots duplicated | PASS — first-occurrence dedup with the documented lexical fold rule |
| 12 | Diagnostics unbounded | PASS — caps on diagnostics AND rejected names, suppressed tails counted |
| 13 | Runtime Failed auto-skips | PASS — EOF policy is an equality against Completed only |
| 14 | Playing vs selected preserved | PASS — two independent markers over the traversal; WYSIWYG pane |
| 15/16 | QUICKSTART vs --help vs ? overlay | PASS — key sets pinned both directions; semantics hand-checked, no contradiction |
| 17 | Packaging alters architecture | PASS — zero changes under playback/composition/output-wasapi/docs/specs/native |
| 18 | New Plugin without D13 | PASS — no ComponentSpec additions |
| 19 | FS op in RT path | PASS — decode IO on the session worker pre-exists; scan probe runs on the App thread in the documented sync-Open stall |
| 20 | Unearned format claims | PASS — QUICKSTART list ⊆ the FFmpeg codec-base closure; the wider prefilter honestly framed |
| 21 | Audibility claimed without human | PASS — no audibility claim anywhere; ACOUSTIC_WITNESS = UNAVAILABLE recorded |
| 22 | Private music committed | PASS — corpus is committed fixtures + declared synthetic; /dist ignored |
| 23 | Third-party recorded | PASS — every shipped crate covered; FFmpeg LGPL pin/sha/recipe/obj-file offer present |
| 24 | Performance phase started | PASS — no profiling/optimization; product diff is hardening + help + packaging |

## Minor findings and fixes applied (this branch)

1. **`std::env::args()` panics on non-Unicode argv** (entry.rs) —
   Windows command lines are UTF-16; an unpaired-surrogate argument
   panicked before the parser could refuse it cleanly.
   FIXED: `args_os()` + lossy conversion (commit edb631b).
2. **Shipped BUILD-MANIFEST cited a dangling commit** — the packaged
   manifest recorded a pre-rebase twin hash not reachable from the
   delivered branch history.
   FIXED: repackaged at final HEAD (commit edb631b and the RUN2
   evidence below carry the corrected identity).
3. **Developer-local absolute paths embedded in the exe** —
   unstripped mingw panic-location metadata carried
   `/home/<user>/...` strings a panic would have shown end users.
   FIXED: the packaging build runs
   `--remap-path-prefix=$HOME/=/qianqian-build/` and FAILS CLOSED if
   any HOME string survives in the binary (verified on the rebuild).

## Observations (recorded, no change required)

- QUICKSTART documents two product semantics the consistency tests do
  not pin (volume step = 5 / range 0–100; `Q` is a literal path
  character inside the O input line). Both match the code today; the
  key-table tests keep the surfaces aligned, semantics drift is
  possible but not observed.
- `dedup_key` is lexical only: trailing-backslash / `..` spellings /
  8.3 short names evade dedup by design and the doc comment says so;
  itemizing every evasion in user-visible docs was judged not worth
  the noise for this release.
- The packaging DLL allowlist is a deliberate superset (a few system
  DLLs the exe does not currently import); the gate still fails closed
  on anything NOT in the list.

## Post-fix evidence (RUN2)

The three fixes changed the shipped binary, so the physical evidence
was RE-RUN against the fixed package (RUN2): extraction at
plain/spaces/CJK, the 6 non-TUI isolated gates, fresh-console
allocation at the CJK path, and the full 13-scenario ConPTY matrix —
see `WINDOWS-TUI-LISTENING-REPORT.md` §3 and
`evidence/ENV-LR1-RUN2.txt`. RUN1 remains in history as the earlier
run of the same scenario set against the pre-fix artifact.
