#!/usr/bin/env python3
"""Tag-push-only publisher. Never edits a tag, release or existing asset."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

import check_release as cr


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--tag', required=True)
    p.add_argument('--source', required=True)
    p.add_argument('--zip', type=Path, required=True)
    args = p.parse_args()
    try:
        cr.publication_policy(os.environ.get('GITHUB_EVENT_NAME'),
                              os.environ.get('GITHUB_REF'), args.tag, False)
        cr.require(os.environ.get('GITHUB_REPOSITORY') == 'jnhu76/qianqian', 'R7 wrong publishing repository')
        args.repo = cr.ROOT
        args.mode = 'formal'
        cr.verify(args)
        # GitHub API failure is TOOLING_FAIL, never interpreted as absence.
        pages = json.loads(cr.run(['gh', 'api', '--paginate', '--slurp',
                                  'repos/jnhu76/qianqian/releases?per_page=100']))
        cr.publication_policy('push', 'refs/tags/' + args.tag, args.tag,
                              any(r['tag_name'] == args.tag for page in pages for r in page))
        refs = cr.run(['git', 'ls-remote', 'origin', 'refs/tags/' + args.tag,
                       'refs/tags/' + args.tag + '^{}']).splitlines()
        mapping = dict(line.split()[::-1] for line in refs)
        peeled = mapping.get('refs/tags/' + args.tag + '^{}', mapping.get('refs/tags/' + args.tag))
        cr.require(peeled == args.source, 'R7 remote tag source mismatch')
        notes = cr.run([sys.executable, str(cr.ROOT / 'tools/check_release.py'), 'notes',
                        '--tag', args.tag, '--source', args.source, '--mode', 'formal', '--zip', str(args.zip)])
        notes_file = args.zip.parent / 'release-notes.md'
        notes_file.write_text(notes + '\n')
        # Unique Release creation refuses races/existing releases. No edit/upload/clobber path.
        subprocess.run(['gh', 'release', 'create', args.tag, str(args.zip),
                        str(args.zip) + '.sha256', str(args.zip) + '.provenance.json',
                        '--repo', 'jnhu76/qianqian', '--verify-tag', '--target', args.source,
                        '--title', 'Qianqian ' + cr.version(args.tag), '--notes-file', str(notes_file)], check=True)
    except cr.PolicyError as exc:
        print('POLICY_RED: ' + str(exc), file=sys.stderr)
        sys.exit(2)
    except Exception as exc:
        print('TOOLING_FAIL: ' + type(exc).__name__ + ': ' + str(exc), file=sys.stderr)
        sys.exit(3)


if __name__ == '__main__':
    main()
