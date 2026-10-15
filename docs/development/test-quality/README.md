# Test-quality campaign: artifact index and evidence delivery record

> **Status:** EVIDENCE INDEX / DELIVERY RECORD. Not an authority.
> The test-process authority is [test-quality-standard.md](../test-quality-standard.md).
> Everything in this directory is execution evidence or a historical campaign record:
> it defines no product behavior, no test obligation and no architecture noun.
> Each record is valid only for its own subject SHA and its own stated scope.

## 1. What is here, and what each artifact is worth

| Artifact | What it is | Truth class | Where its subject/limits are stated |
|---|---|---|---|
| `r2-pilot-report.md` | R2 three-pilot adequacy report (PR #250) | evidence / campaign record | own header; the PR body records HEAD as a pending placeholder by design |
| `r2-checker-calibration.md` | opt-in checker calibration (10 TP / 5 FP on 15 cases) | evidence / calibration | own header, `SUBJECT_SHA = 056a01dab311dcffc2ca5a51fbf645d9603423c3` |
| `r3-wave-a-report.md` | R3 Wave A oracle repairs (PR #254, merged as `f3eb7587`) | evidence / campaign record | own report; includes the `EVIDENCE_LOST` gate incident |
| `h0-*.md`, `h0-*.csv` | H0 adversarial remediation campaign (#260 / PR #261 lineage, method case #262) | historical evidence | each file carries its own stage banner; older APPROVE/debt lines do not describe the later HEAD |
| `h1-c4c5-seal-witness.md` | H1 C4/C5 seal-witness evidence | evidence | own section on compiled-artifact hashes and replay |
| `hd-*.md`, `hd-disposition-ledger.csv` | ADR-driven H-D dispositions | evidence | own header, `READ_ONLY_SUBJECT_SHA = bc0b317b4f502f986d8dc480b8b0d4e29c7bf525` |
| `evidence/SHA256SUMS` | digests for the archived packages | evidence index | see the gap in section 2 |
| `evidence/README.zh.md` | archive semantics and the limits of the H0 packages | evidence index | own text |

## 2. In-repository evidence packages (`evidence/`)

```text
h0-independent-audit.zip           1,316,164 B   digest recorded in SHA256SUMS
h0-adversarial-remediation.zip     1,893,292 B   digest recorded in SHA256SUMS
hd-initial-evidence.zip           18,406,453 B   NO entry in SHA256SUMS
h1-c4c5-initial-evidence.zip          64,998 B   NO entry in SHA256SUMS
SHA256SUMS                                188 B   2 lines, both h0 packages
```

Recorded gap: `evidence/SHA256SUMS` covers only the two H0 packages. The `hd-` and
`h1-c4c5-` packages are in the tree without an entry there, so their in-repo integrity
can be checked only from the manifests inside each package (and from Git object
integrity). This is a delivery-hygiene gap, not a claim that either package is
corrupt; closing it means adding their digests, which changes a record that the
existing H0 record points at, so it is left as a declared gap rather than silently
edited.

Aggregated-subprocess-trace limits for the H0 packages are stated in
`evidence/README.zh.md` and are not repeated here.

## 3. Not in the repository: the R3-A and R3-B portfolios

The two full portfolio records live as GitHub issue comment payloads. They are
durable, but they are tracker-bound: they do not exist in the working tree.

```text
#252  R3-A census archive, 893 rows, subject 3664337f0b54b9badaeff8fc25aab8da59a90472
#255  R3-B rebaseline, 914 rows + 16 report/ledger payload comments,
      subject f3eb75874c6e3e8e1560a4a098782229227dda33
      independent review: PASS_WITH_DECLARED_GAPS (issuecomment-6120289277)
```

Two derived facts from those records are canonical for any future ledger work and are
adopted in the standard:

```text
IDENTITY   the stable key is TEST_ID + TARGET + PLATFORM + CFG_FEATURE.
           A bare TEST_ID is not unique: 914 records carry 913 distinct TEST_IDs
           (one cfg-gated symbol is recorded twice, for Linux default and Windows cfg).
VERDICTS   the legacy ADEQUATE(screening-depth) string is retired. The canonical
           derived view is screening_result = SCREENED_NO_CONFIRMED_DEFECT with
           adequacy_verdict = NOT_ASSESSED. Only the exact value ADEQUATE counts in an
           adequate rollup.
```

## 4. R3-B raw evidence archive: identity, status and pickup contract

```text
package    qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz
subject    f3eb75874c6e3e8e1560a4a098782229227dda33
bytes      506,716 compressed (3,497,480 uncompressed)
sha256     1f095af98097c0c836c711eaaa7c651f8b75926c4fe66b2801e9d703473f5bf8
members    225 (67 directories, 158 regular files)
per-file   157 OK / 0 mismatched; 157 = 158 - 1, the uncovered file being
           sha256sums.txt itself, which cannot carry a fixed point of its own hash
runs       61 RUN_ID directories, 124 files under runs/, 496,280 log bytes
claimed by the packaging record, never by a second party
contents   manifest.json (61 runs with recorded and re-verified on-disk digests),
           environment.json, sha256sums.txt, runs/<RUN_ID>/{stdout-stderr.log,
           metadata.json}, results/*.csv, ci/{github-run-index.json,
           downloaded-logs/*.log, issue-252-comments-raw.json}, reports/
pickup set package + .sha256 + R3B-ARCHIVE-ADDENDUM-verification-notes.md
           + R3B-ARCHIVE-ADDENDUM-command-reconstruction.csv
decision   (c) private direct handover first: do not cut a product release to publish
           test logs, do not add a binary blob to repository history, and do not
           publish unredacted local paths by default
```

Disclosed defect in the frozen package: `capture-run.sh` v1 built `argv` and
`command_string` inside its `finalize()` function, where a shell `"$@"` resolves to the
positional parameters of that function (empty). All 61 metadata records therefore carry
an empty argv entry and a command string with no program text -- the COMMAND field is
missing, although labels, subject SHA, worktree path, executable path, exit code,
outcome, timestamps, platform, toolchain versions, `Cargo.lock` digest and log digests
are real and self-consistent. The harness was fixed after the package was frozen (freeze
the command vector at top level and pass it into `finalize`); the frozen archive must not
be rewritten.

Reconstruction shipped beside the package: one row per RUN_ID with a confidence class
and an independent cross-check against the recorded executable path.

```text
rows            61/61
confidence      EXACT_SCRIPT 45  |  EXACT_REPORT 12  |  PARTIAL_ARGV0_ONLY 4
cross-check     58/61 MATCH; 3 n/a (the deliberate harness self-tests)
recorded field  empty on all 61 rows -- the reconstruction is separate evidence and is
                never presented as the recorded value
```

### Delivery status, including the 2026-10-15 closeout finding

```text
AT RECORD TIME   ARCHIVED_LOCALLY / NOT_PUBLISHED / NOT_INDEPENDENTLY_HASH_VERIFIED
CLOSEOUT FINDING the recorded local roots DO NOT EXIST on the operator machine used for
                 this closeout:
                   /home/hoo/Source/qianqian-r3b-artifacts   ABSENT
                   /home/hoo/Source/qianqian-r3b-evidence    ABSENT
                   /home/hoo/Source/qianqian-r3b-reports     ABSENT
                   /home/hoo/Source/qianqian-r3b-tools       ABSENT
                   /home/hoo/Source/qianqian-r3b-audit       ABSENT
                   /home/hoo/Source/qianqian-r1-reports      ABSENT
                 (bounded home-directory search plus the recorded paths; no matching
                 tarball found)
TRACKER SCAN     an exhaustive scan of this repository tracker on 2026-10-15 (all 279
                 issue/PR items: bodies, issue comments and PR review comments) found
                 NO uploaded attachment anywhere: no user-attachments or
                 githubusercontent object URL, only unrelated CodeRabbit badge images.
                 A cross-repository search over the account for the package name and
                 for its sha256 returns text mentions only. Discussions are disabled
                 and neither release carries this asset. The package was therefore
                 never published through the tracker, and a reader should not expect
                 to find it there.
PRECEDENT        the later H0 / H1 / H-D rounds did deliver their evidence packages,
                 by committing them under docs/development/test-quality/evidence/
                 (largest: hd-initial-evidence.zip, 18,406,453 B). That channel exists
                 and is proven; decision (c) above deliberately did not use it, so
                 revisiting that decision is the only route that needs no new
                 infrastructure.
OWNER RULING     the archive is recorded as LOST_PERMANENTLY (owner ruling,
                 2026-10-15): it was never published, the recorded local roots are
                 gone, and it is not present in the tracker. The issue is closed on
                 that basis with the loss declared, not with a delivery claim.
CURRENT STATUS   LOST_PERMANENTLY (the identity below is preserved so that the loss
                 stays documented and any future copy can still be checked against it)
CONSEQUENCE      the DURABLE artifacts available today are the GitHub comment payloads
                 (reports, inventories, ledgers, the reconstruction table) and the
                 identity record above. The raw logs are not retrievable here, and no
                 claim in this repository depends on their bytes.
```

### Verification recipe for whoever holds the archive

```bash
stat -c %s qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz        # expect 506716
sha256sum qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz         # expect 1f095af9...73f5bf8
tar tzf qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz | wc -l   # expect 225
tar tzf qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz | grep -vc "/$"   # expect 158
tar xzf qianqian-r3b-test-evidence-f3eb75874c6e.tar.gz
sha256sum -c sha256sums.txt                                      # expect 157 OK / 0 failed
ls runs/ | wc -l                                                 # expect 61
find runs -type f | wc -l                                        # expect 124
```

Only a second party running this recipe can promote the status to
`DELIVERED_AND_VERIFIED`. Until then, the package must not be described as delivered,
published or verified, and the missing COMMAND field stays a defect of the frozen
record rather than a gap filled by the reconstruction.

## 5. What a reader may and may not conclude

```text
MAY      inventory counts, size/scope/risk distributions and review depths as
         reported at their subject SHA; the composite identity key; per-run execution
         results as recorded facts; the declared gaps and limits.
MAY NOT  that any R3-B mutation was executed (zero were; FALSIFIER_EXECUTED appears on
         zero rows); that any finding is CONFIRMED_RUNTIME (zero are); that
         ADEQUATE = 233 means mutation-confirmed or cross-platform assured; that
         Windows coverage is complete (13 cfg-gated out, 216 not selected, and
         qianqian-output-wasapi / -audio-api / -composition / -app never execute
         there); that the raw logs were verified by a second party; that a hidden local
         path still holds evidence; or that a green suite is architecture acceptance.
```

## 6. Reproduction entry points

The runner commands, RUN_IDs and CI job logs are indexed in the R3-B report payloads
(`r3b-evidence-index.md`, `r3b-execution-summary.md`) and in `hd-execution-evidence.md`
for the H-D round. The record-keeping rules that make a rerun comparable -- immutable
per-run logs, a captured command vector, honest result classes, write/fetch/compare for
tracker writes -- are normative in
[test-quality-standard.md](../test-quality-standard.md) section 12.
