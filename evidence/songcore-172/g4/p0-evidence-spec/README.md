# G4 evidence archive — location, minimal shape, receipt convention (p0 evidence-spec)

```text
TRUTH CLASS   Non-authoritative filing convention for the #172 G4 evidence round.
              It defines no release policy and grants no maturity state.
              evidence/README.md:3 ("non-authoritative evidence collected from
              outside Qianqian") governs the tree; the release contract §11
              (docs/release/songcore-release-contract.md:563–624) governs what
              each G4 gate must prove. This file only fixes where the proof
              files go and what the minimum per-directory set is.
SOURCE SHA    69ba1ce05a2772f5aa077c6ece3365c40f6e0500 (main, tracked tree clean
              at the time of this audit; only untracked evidence/songcore-172/g4/
              exists).
SCOPE         read-only audit of evidence/README.md, docs/README.md,
              docs/architecture/registry.yml, CONTEXT.md,
              docs/release/songcore-release-contract.md, docs/release/pr-a-body.md,
              evidence/songcore-172/q2-crossroot/README.md,
              evidence/songcore-172/package/package-summary.json,
              evidence/songcore-172/linux/build/s3-build.md,
              evidence/native-tier2-296/RECEIPTS.sha256, AGENTS.md.
              No build, gate, release-policy or lefthook run was executed here.
```

## 1. Evidence root

```text
evidence/songcore-172/g4/
```

One directory per G4 lane, plus this `p0-evidence-spec/` convention lane.
Nothing in this round is written outside `evidence/songcore-172/g4/<lane>/`.

## 2. Directory set

Normative for this round (this is the set the final G4 report may cite):

```text
build-linux/        G0 for linux-x86_64 (static + shared where the recipe declares it)
build-windows/      G0 for windows-x86_64 (mingw-w64 cross, static-only per §6)
package/            exact content set §7, manifest schema-2 V1–V7 §8,
                    checksums.txt ↔ files[] equality, symbols.txt audit
consumers/          G1 (minimal C consumer link + songcore_abi_version()==1)
                    and G2 (§11:586–598 scenario on the target's real runtime);
                    G3 only if a binding ships for the target
dependency-audit/   §9 runtime-dependency audit fresh for this artifact per
                    target (NEEDED set, undefined-symbol census, link line)
licensing/          §10 O1–O4 / B1–B4 (pinned-source COPYING texts hashes,
                    THIRD_PARTY_NOTICES, LICENSE-MIT/APACHE identity, V5 args)
release-policy/     repository-side gates this round actually ran
                    (tools/test_release.py E01–E18, docs-verify route,
                    lefthook pre-push) — or their NOT_RUN record
reconcile/          cross-lane reconciliation: five-state per-target status,
                    historical-release immutability check, NOT_RUN register,
                    root RECEIPTS.sha256
reviews/            fresh-context review verdicts over the other lanes
p0-evidence-spec/   this convention (lane id p0)
```

Observed on disk at the source SHA (recorded, not renamed — this lane is
read-only):

```text
p0-contract/        (empty at the time of this audit)
p0-entries/
p0-governance/
d1-out-handle/
```

These are the pre-flight/governance lanes of the same round. The `reconcile/`
lane must map them into the final G4 report index (or the round owner renames
them); this file does not rename another lane's directory.

## 3. Minimum file set — every lane

Exactly these are mandatory; everything else is optional and must lower the
information content, not raise the volume:

```text
README.md       truth class, scope, source SHA, commands of record (argv),
                verdict, and an explicit NOT_RUN section
stage.json      stage_id / source_sha / input_sha / target_toolchain /
                argv[] / exit_codes[] / evidence_paths[] / evidence_sha256[] /
                verdict / not_run[]
drivers/        byte-identical copies of the scripts that produced the logs,
                with a sha256 table in README (precedent:
                evidence/songcore-172/q2-crossroot/README.md:301–316)
logs/           one file per executed command or run, UTC-stamped names
                (precedent naming: evidence/songcore-172/suite-results/20261029-051408/)
verdict.json    machine verdict; for gate receipts the schema-tagged form is
                already in-tree (schema `songcore-release-gate-g1g2/1`,
                evidence/songcore-172/q2-crossroot/gates/receipts/<root>/gate-*-receipt.json)
```

A lane that executes nothing still owes `README.md` + `stage.json` with
`verdict: NOT_RUN` and a `not_run[]` naming the missing prerequisite.

## 4. Per-directory minimum content

```text
build-linux/, build-windows/
    build receipt (JSON): source HEAD/tree, clone provenance, closure identity
        (manifest sha256, target, toolchain, recipe_sha256), xmake argv + exit
        codes, artifact path/sha256/bytes
    member-format audit: `objdump -f` census text (280/280 elf64-x86-64 or
        pe-x86-64, 0 foreign) — text only, never the archive
    drivers/ + logs/
    NOT_RUN: any form/route the lane did not build

package/
    package-summary.json (field set as evidence/songcore-172/package/package-summary.json:
        script, version, source{commit, abi_header_sha256}, audits, checks,
        container{zip_sha256, entry_count}, artifact_sha256/bytes)
    v-checks.json (V1–V7 + O1 + B4 booleans, precedent
        evidence/songcore-172/package/v-checks.json)
    SHA256SUMS sidecar of the container, extracted metadata/manifest.json,
        metadata/checksums.txt and metadata/symbols.txt as TEXT copies
    the package binary itself (.zip/.tar.gz) is NOT committed: it is identified
        by sha256 + entry count + the publication location

consumers/
    consumer source (canonical corpus path, §11:586–598), compile+link argv
        with exit codes, runtime log per target, and a summary.json with the
        scenario-step verdicts; windows link-line proof (-lbcrypt) lives here
        or in dependency-audit/ (one owner, not both)

dependency-audit/
    readelf -d NEEDED / objdump -p import census per shipped form, nm census
        of undefined symbols per member, and the minimal-consumer link-line
        positive and negative controls (precedent shape:
        evidence/songcore-172/audit/{lin,win}-link-*.log +
        *-undefined-by-member.txt)

licensing/
    sha256 of each shipped COPYING text vs the pinned-source extraction,
        THIRD_PARTY_NOTICES.txt hash, LICENSE-MIT/LICENSE-APACHE vs repo-root
        texts at the source commit, configure_args V5 check result

release-policy/
    tools/test_release.py E01–E18 result (report JSON + exit code), docs-verify
        log + meta (route used), lefthook pre-push log — each file carries the
        argv, the host, and the exit code; anything not run goes to not_run[]

reconcile/
    status.json: per target, the five-state maturity word plus
        target/toolchain/bounds/result-class (AGENTS.md:385–393)
    immutability check for songcore-v0.1.0 and the legacy RC
        (7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415,
        evidence/songcore-172/q2-crossroot/README.md:333–336)
    NOT_RUN register aggregated from every lane
    RECEIPTS.sha256 over the whole g4/ tree (see §5)

reviews/
    one verdict file per fresh-context review, each citing the lane files and
        their sha256 it reviewed; no new gate claims
```

## 5. Receipt and hashing convention

```text
per lane        stage.json (schema above) — this round's requirement
per run         logs/<YYYYMMDD-HHMMSS>-<slug>.{log,json,txt}
                plus exit code inside the log's first line or stage.json
root            evidence/songcore-172/g4/RECEIPTS.sha256
                format: `sha256sum` output, sorted, `./`-relative paths,
                covering every committed file under evidence/songcore-172/g4/
                (including nested lane receipts)
                written LAST by the reconcile lane, after all lanes settle
```

Precedent for the root manifest: `evidence/native-tier2-296/RECEIPTS.sha256`
(sha256 `ba9ab02a…1735`, 56 lines, `./`-relative, and it hashes nested raw
receipts too).

Stated honestly: `evidence/README.md` itself defines **no** file-naming or
receipt convention — it has only "Default loading rule" (line 7), "Authority
boundary" (line 21) and "External systems" (line 33). The convention above is
in-tree precedent (q2-crossroot build receipts/gate receipts, native-tier2-296
RECEIPTS.sha256, package SHA256SUMS) plus this round's stage.json rule; it is
not authority and does not change `evidence/README.md`.

## 6. Forbidden to commit in this round

```text
shipped artifacts and containers        libsongcore.a / libsongcore.so / DLL /
                                        .zip / .tar.gz of any SongCore package
complete build trees                    native/build/**, build/ffmpeg-xmake/**
                                        (replayed closure), .xmake/**, any
                                        per-root clone
object / closure caches                 *.o / *.obj, libqianqian_av.a, raw
                                        compile_commands.json (archive only the
                                        derived census: entries count and
                                        prefix-map counts, as q2-crossroot
                                        fix-witness does)
whole machine logs                      raw multi-MB logs; commit the filtered
                                        excerpt (<= 256 KiB per file), the exit
                                        codes, and the sha256 + byte count of
                                        the unabridged log kept off-repo
environment residue                     gh auth/tokens, node_modules, website
                                        build output, %TEMP% workdirs, absolute
                                        paths that leak secrets
cross-lane duplication                  the same raw log copied into two lanes
```

Precedent conflict, recorded rather than hidden: this repository already
tracks two package-size binaries —
`evidence/songcore-172/linux/verify/package/songcore-v0.0.0-s3-verify-linux-x86_64.tar.gz`
(2,593,935 bytes, blob `1642c584`) and
`evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip`
(1,448,965 bytes, blob `56d9d883`), both confirmed with
`git ls-files -s` at the source SHA. The "no artifacts" rule above is this
round's rule for G4 evidence; it does not retroactively rewrite those
archives. If the round owner wants a hard size ceiling, it has to be stated as
policy — this file records the tension, it does not resolve it.

### 6.1 In-flight observation at audit time (untracked, pre-commit)

`git status --porcelain --untracked-files=all` at the source SHA shows the
rule in §6 is already under pressure inside this round's own tree. Present
but untracked (nothing committed yet, so this is a warning, not a history
change):

```text
3,309,294  evidence/songcore-172/g4/build-windows/libsongcore.a.wsl-copy
3,309,294  evidence/songcore-172/g4/p0-entries/package-rc-out/default/stage/
           songcore-v0.2.0-rc.1-windows-x86_64/lib/libsongcore.a
1,449,651  evidence/songcore-172/g4/p0-entries/package-rc-out/default/
           songcore-v0.2.0-rc.1-windows-x86_64.zip
  347,635  evidence/songcore-172/g4/build-windows/nm.txt   (whole-symbol dump;
           exceeds the 256 KiB text cap in §6 — archive the 15-symbol ABI set
           and the undefined census instead)
```

`p0-entries/package-rc-out/` is a copied staging tree (`.../stage/<pkg>/lib/...`),
which is exactly the "complete build tree" class. The owning lanes must drop
these paths (or record them off-repo by sha256) before the round commits; the
`reconcile` lane's RECEIPTS.sha256 must be generated over the cleaned tree
only.

Also observed: `evidence/songcore-172/build/rc/20261030-203045/` (10 files)
was written outside the `g4/` root during this audit. `build/rc/` is an
existing tracked convention (66 tracked files, e.g.
`build/rc/20261029-041837/summary.json`), so this is a lane-root deviation
from this round's "write only under g4/<lane>/" rule, not a new shape.

## 7. Status vocabulary the final report must keep apart

Three classes, never merged (AGENTS.md:385–393 "Verification", :395–473
"Local pre-push gate"):

```text
HISTORICAL_PASS     executed in an earlier session and archived. Cite the
                    archive path + sha256 + the original date. Never restate
                    it as this round's execution.
EXECUTED_THIS_ROUND executed in this round. Cite argv, exit code, log path,
                    host/target/toolchain, and the result class.
NOT_RUN             did not execute. Literal token `NOT_RUN` — never PASS,
                    never "should pass", never an empty slot.
```

NOT_RUN writing requirements (AGENTS.md:429–433, :457–470):

```text
- the word NOT_RUN appears in the lane README and in stage.json not_run[]
- the missing prerequisite is named (host tool, credential, permission,
  artifact, decision) — "not run" alone is not acceptable
- a gate after the first red gate of the same piped lefthook run is NOT_RUN
- NOT_RUN is scoped: one target's or one route's PASS never cancels another
  target's or route's NOT_RUN
- NOT_RUN is never promoted to PASS by CI history or by a sibling lane
```

## 8. Routing and terminology audit (recorded only, nothing changed)

```text
1. Release contract IS routed.
   docs/README.md:17 routes `release/songcore-release-contract.md` as
   OWNER-RATIFIED (Issue #172 S1; ratified 2026-10-30 by the explicit approval
   PR reviewed under Issue #330; §14 Q1/Q4/Q5 reserved). The contract header
   (docs/release/songcore-release-contract.md:3–23) states the same ratification
   act and the same reservations. Consistent. docs/README.md:87–92 repeats it
   in the authority model block.

2. registry.yml ARCH-008 evidence list is stale.
   docs/architecture/registry.yml:220–248 declares ARCH-008 status CURRENT,
   decisions #172/#330, implementation pr 327, and exactly two evidence
   entries (package-summary.json, verdicts/review-C.md). It does not list the
   S3 linux archive (evidence/songcore-172/linux/build/s3-build.md, commit
   b49b18c) nor the #329 cross-root archive
   (evidence/songcore-172/q2-crossroot/README.md, commit 69ba1ce), although
   both implement the contract it routes. Routing gap, not a contradiction.

3. CONTEXT.md is silent on SongCore release status, not contradictory.
   `grep -n "SongCore\|songcore" CONTEXT.md` returns lines 13, 14, 509, 517,
   557 only — binding-architecture routing and the production workspace list.
   There is no #172 / #329 / #330 / release-contract / v0.1.0 / v0.2.0
   statement, so nothing there can contradict the #329/#330 conclusions;
   the release-status omission is recorded as a gap only.

4. `evidence/` truth-class framing vs this subtree.
   evidence/README.md:3 declares the tree "evidence collected from outside
   Qianqian", yet evidence/songcore-172/** is Qianqian's own release evidence
   (q2-crossroot/README.md:3–7 states it is archived under the evidence/ rules
   as non-authoritative). The non-authoritative half holds; the "external"
   half does not describe this subtree. Terminology tension, recorded.

5. `docs/release/pr-a-body.md` is unrouted historical PR text.
   - 239 lines, Chinese, title "fix(songcore): make Windows static target builds
     deterministic and fail-closed (#172)"; its own header (lines 3–8) records
     PR-A as fail-closed on this host on 2026-10-29, never pushed, never opened,
     retained as a draft body, with the E14 real-ELF platform assumption still
     needing an authority decision.
   - Committed by 742abf8 ("build(songcore): establish Windows x86_64
     release-candidate lane (#327)"); `git log --follow` shows no later touch.
   - References: none from docs/README.md, docs/architecture/registry.yml, or
     docs/release/songcore-release-contract.md (`grep -rn "pr-a-body"` — the
     contract has zero hits). Only cited from
     evidence/songcore-172/verdicts/{pr-a-close.md:9,91, review-C.md:353,
     s2-package.md:391}.
   - So: historical PR-body residue living in the `docs/release/` directory
     whose routed content is only the contract. Recorded; no move made.
```

## 9. What this lane did not do (NOT_RUN)

```text
NOT_RUN  no G0–G4 gate was executed here (read-only specification lane)
NOT_RUN  no build, xmake, objdump, readelf, nm or consumer link was run
NOT_RUN  no WSL invocation; this audit ran entirely in Windows Git Bash
NOT_RUN  no lefthook pre-push and no tools/test_release.py (not required for a
         documentation-only lane; the round's release-policy lane owns them)
NOT_RUN  no RECEIPTS.sha256 was generated: other g4 lanes are still writing;
         the reconcile lane generates it last
python3  invoked once for JSON key extraction and failed with exit 49
         (WindowsApps stub, measured this session); node exited 0 and supplied
         the one JSON key listing used. No python3 output informs any claim.
```
