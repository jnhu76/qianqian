# Commit and PR title convention

Qianqian uses one repository-wide commit header format:

```text
<type>(<scope>): <subject>
```

Breaking changes use:

```text
<type>(<scope>)!: <subject>
```

The same format is required for pull-request titles.

The machine-readable policy lives in `.github/commit-convention.json`. The checker is `tools/check_commit_convention.py`, and the Lefthook `pre-push` gate (see `lefthook.yml`, `AGENTS.md`, `CONTRIBUTING.md`) enforces the policy locally on every push.

## Allowed types

Only one primary type is allowed per commit.

| Type | Use it for |
| --- | --- |
| `feat` | A new product/system capability |
| `fix` | A real bug fix |
| `refactor` | Code-structure change without intended behavior change |
| `perf` | Performance-oriented implementation or optimization |
| `test` | Tests, executable evidence, oracles, and negative controls |
| `docs` | Documentation-only changes |
| `build` | Build system, dependency, toolchain, linking, or packaging changes |
| `ci` | CI/workflow behavior itself |
| `chore` | Repository maintenance that does not fit another type |
| `revert` | An intentional revert |

Examples:

```text
feat(plugin): add SongCore decode provider
fix(native): preserve SongSource callback lifetime
refactor(runtime): remove hardcoded output registration
perf(ffi): reduce decoder call overhead
test(kernel): prove provider withdrawal ordering
docs(perf): record decode cost model
build(native): link SongCore static closure
ci(review): enable draft PR review
chore(repo): define commit convention policy
revert(runtime): restore previous composition behavior
```

Do not combine types:

```text
ci feat: add decoder
feat ci: add decoder
chore/fix(native): fix decoder
test+fix(native): fix decoder
```

If one cohesive change touches implementation plus a small amount of documentation or CI glue, choose the type that describes the primary repository change. Split commits when the changes have independent purposes.

## Scope

Scope names describe a real responsibility/domain and use lowercase kebab-case.

Good scopes include:

```text
plugin
native
ffi
runtime
kernel
wasapi
perf
repo
review
```

Do not use roadmap or project-management labels as scopes:

```text
E2
B4
phase-e
stage-2
round-3
```

A scope is required. This keeps `git log --oneline` useful and makes ownership visible without reading the body.

## Subject

The subject:

- begins with a lowercase ASCII letter or digit;
- states the action/result rather than the project phase;
- has no trailing full stop;
- keeps the complete header at 72 characters or fewer.

Prefer an imperative/action phrase:

```text
feat(plugin): add SongCore decode provider
```

Avoid status prose:

```text
feat(plugin): Added the new SongCore decode provider.
```

Issue/PR references belong in the body/footer when useful, not in the subject:

```text
fix(native): preserve SongSource callback lifetime

Refs #123
```

## `ci`, `build`, and `chore`

These types are intentionally narrow.

Use `ci` when the primary change is CI behavior, usually under `.github/workflows/**` or CI-specific tooling. A feature that merely adds a CI check remains a `feat` when the feature itself is the primary change.

Use `build` for build-system and toolchain mechanics such as Xmake/Cargo dependency wiring, link configuration, packaging, or compiler configuration.

Use `chore` only as a maintenance fallback. Do not hide features, bug fixes, refactors, or performance work under `chore`.

## Breaking changes

Use `!` only for an intentionally breaking contract change:

```text
feat(plugin)!: replace decoder capability contract
```

When useful, explain the break in the commit body with a `BREAKING CHANGE:` footer.

## Pull-request titles

PR titles follow exactly the same header rule as commits:

```text
feat(plugin): add real SongCore decode plugin
```

The PR body may contain campaign names, evidence identifiers, issue numbers, and detailed status. The title should stay semantic and durable.

The local gate cannot see a PR title (it does not exist locally); titles are author-checked. Deriving the title from the commit header keeps them identical by construction.

## Automated gate

The Lefthook `pre-push` gate validates, on every push:

1. the checker itself against known valid/invalid examples (`--self-test`);
2. every commit header ahead of the push base
   (`--range origin/main HEAD`) — the set a push from this ref would
   carry; an empty range passes, because there is nothing local to
   validate.

Canonical manual run:

```bash
lefthook run pre-push --all-files
```

or the check directly:

```bash
python3 tools/check_commit_convention.py --self-test
python3 tools/check_commit_convention.py --range origin/main HEAD
```

There is no server-side commit-convention workflow anymore: enforcement
is local, before the commits exist to push. When the gate fails, fix the
header directly (`git commit --amend`, or `git rebase -i` for older
commits) and re-run the gate — never push around it. A push that
bypasses the hook (`git push --no-verify`) is not machine-checked
anywhere.

The policy is configured in `.github/commit-convention.json` and implemented with Python standard-library code only; no Node/commitlint dependency is required.

The governing principle is:

> **Type says what the commit does to the repository; scope says which responsibility/domain it changes.**

Neither field should encode roadmap stage, experiment number, or temporary campaign status.
