#!/usr/bin/env python3
"""Issue #153: check the boundary workflow's restricted positive path profile.

Not a YAML parser or a complete GitHub glob emulator: quoted lists, literals,
segment '*' and terminal '/**' only. Other syntax fails closed for review.
This checks source trigger reachability, not an executed GitHub single-file PR.
"""
import argparse
import ast
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / '.github/workflows/plugin-boundary-gate.yml'
PROBES = ('crates/qianqian-audio-api/build.rs',
          'apps/headless/src/tui/model/projection.rs')


def block(text, key, indent):
    lines = text.splitlines()
    marker = ' ' * indent + key + ':'
    starts = [i for i, line in enumerate(lines) if line == marker]
    if len(starts) != 1:
        raise ValueError(f'expected exactly one literal block {marker!r}')
    result = []
    for line in lines[starts[0] + 1:]:
        if line.strip() and not line.lstrip().startswith('#'):
            if len(line) - len(line.lstrip()) <= indent:
                break
        result.append(line)
    return '\n'.join(result)


def quoted_patterns(text):
    patterns = []
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        match = re.fullmatch(r'''\s*- ("[^"]+"|'[^']+')\s*(?:#.*)?''', line)
        if not match:
            raise ValueError(f'unsupported path-list syntax: {line!r}')
        patterns.append(ast.literal_eval(match[1]))
    if not patterns:
        raise ValueError('empty path list')
    return patterns


def matches(pattern, path):
    if (not re.fullmatch(r'[\w./*\-]+', pattern)
            or ('**' in pattern and not pattern.endswith('/**'))
            or '**' in pattern[:-2]):
        raise ValueError(f'unsupported positive path pattern: {pattern!r}')
    expression = re.escape(pattern).replace(r'\*\*', '.*').replace(r'\*', '[^/]*')
    return re.fullmatch(expression, path) is not None


def profiles(workflow):
    events = block(workflow, 'on', 0)
    return {event: quoted_patterns(block(block(events, event, 2), 'paths', 4))
            for event in ('pull_request', 'push')}


def check(found, local):
    errors = []
    # Exact containment ensures all future files admitted by the local glob are
    # included, rather than relying only on a handful of example filenames.
    required = set(local) | {'lefthook.yml', '.github/workflows/plugin-boundary-gate.yml',
                             'tools/check_plugin_boundary_triggers.py'}
    for event, patterns in found.items():
        for pattern in patterns:
            matches(pattern, PROBES[0])  # Validate even unused profile entries.
        for pattern in sorted(required - set(patterns)):
            errors.append(f'{event}: missing local/config input pattern {pattern}')
        for path in PROBES:
            if not any(matches(pattern, path) for pattern in patterns):
                errors.append(f'{event}: missed single-file change {path}')
    return errors


def negative_controls(found, local):
    # Restore the two legacy root profiles independently. These mutations model
    # precisely the old filters that omitted build.rs and all App source.
    for path, current, legacy in (
        (PROBES[0], 'crates/**', ['crates/*/Cargo.toml', 'crates/*/src/**']),
        (PROBES[1], 'apps/**', ['apps/*/Cargo.toml']),
    ):
        mutant = {event: [p for p in patterns if p != current] + legacy
                  for event, patterns in found.items()}
        errors = check(mutant, local)
        for event in found:
            diagnostic = f'{event}: missed single-file change {path}'
            if diagnostic not in errors:
                raise ValueError(f'negative control did not get target RED: {diagnostic}')
            if not any(matches(pattern, path) for pattern in found[event]):
                raise ValueError(f'positive profile does not cover {event}: {path}')
        print(f'RESULT trigger-{path} RED (legacy profile misses both events; current covers)')


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--workflow', type=Path, default=WORKFLOW)
    parser.add_argument('--negative-controls', action='store_true')
    args = parser.parse_args()
    try:
        hook = (ROOT / 'lefthook.yml').read_text()
        local = quoted_patterns(block(block(block(block(hook, 'pre-push', 0),
                    'commands', 2), 'plugin-boundary', 4), 'glob', 6))
        found = profiles(args.workflow.read_text())
        errors = check(found, local)
        if errors:
            print('\n'.join('TRIGGER_COVERAGE_RED: ' + error for error in errors))
            return 1
        if args.negative_controls:
            negative_controls(found, local)
        print('PLUGIN_BOUNDARY_TRIGGERS: PASS (pull_request + push; local input parity)')
        return 0
    except (OSError, ValueError, SyntaxError) as error:
        print(f'TOOLING-FAIL: {error}')
        return 2


if __name__ == '__main__':
    sys.exit(main())
