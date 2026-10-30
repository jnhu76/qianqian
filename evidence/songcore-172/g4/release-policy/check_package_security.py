#!/usr/bin/env python3
"""Read-only release-archive content audit (Lane F, issue #172 G4 precheck).

Checks, per archive, with no extraction to disk and no writes outside the
caller-supplied --out directory:

  T1  path traversal: absolute path, drive-letter/UNC prefix, '..' component,
      backslash separator, empty name, NUL
  T2  duplicate member names
  T3  symlink / hardlink / non-regular member (dir is reported, not flagged)
  T4  abnormally large member (>= 100_000_000 bytes, same bound as
      tools/check_release.py R4)
  T5  top-level entries outside the expected single versioned root
  T6  archive identity: sha256 + size, compared against an expected value if
      supplied, plus a hash of the sorted member manifest

Usage: check_package_security.py ARCHIVE --expected-root NAME
       [--expected-sha256 HEX] [--out OUT.json]
Exit: 0 all checks clean, 2 a security finding, 3 tooling/usage failure.
"""
import argparse
import hashlib
import json
import posixpath
import stat
import sys
import tarfile
import zipfile

MAX_MEMBER = 100_000_000


def sha256_stream(fh, chunk=1 << 20):
    h = hashlib.sha256()
    while True:
        b = fh.read(chunk)
        if not b:
            break
        h.update(b)
    return h.hexdigest()


def path_problems(name):
    out = []
    if name == '':
        out.append('empty member name')
        return out
    if '\x00' in name:
        out.append('NUL in member name')
    if '\\' in name:
        out.append('backslash separator')
    if name.startswith('/') or name.startswith('//'):
        out.append('absolute path')
    if len(name) > 1 and name[1] == ':':
        out.append('drive-letter path')
    parts = [p for p in name.replace('\\', '/').split('/') if p not in ('', '.')]
    if any(p == '..' for p in parts):
        out.append("'..' component")
    return out


def audit_zip(path, expected_root):
    members = []
    findings = []
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        seen = {}
        for i in infos:
            problems = path_problems(i.filename)
            mode = i.external_attr >> 16
            kind = 'dir' if i.is_dir() else ('symlink' if stat.S_ISLNK(mode) else 'file')
            entry = {'name': i.filename, 'size': i.file_size, 'compressed': i.compress_size,
                     'kind': kind, 'mode': oct(mode)}
            if kind == 'file':
                entry['sha256'] = hashlib.sha256(z.read(i)).hexdigest()
            members.append(entry)
            for p in problems:
                findings.append({'check': 'T1_traversal', 'member': i.filename, 'detail': p})
            if kind == 'symlink':
                findings.append({'check': 'T3_symlink', 'member': i.filename,
                                 'detail': 'ZIP member has S_IFLNK mode, links to '
                                           + repr(z.read(i)[:200])})
            if i.file_size >= MAX_MEMBER:
                findings.append({'check': 'T4_oversize', 'member': i.filename,
                                 'detail': f'{i.file_size} bytes >= {MAX_MEMBER}'})
            key = i.filename
            seen[key] = seen.get(key, 0) + 1
        for name, n in sorted(seen.items()):
            if n > 1:
                findings.append({'check': 'T2_duplicate', 'member': name,
                                 'detail': f'{n} entries share this exact name'})
    return members, findings


def audit_tar(path, expected_root):
    members = []
    findings = []
    # tarfile's own safety (absolute/'..') is not relied on: we only list and
    # hash, never extract.
    with tarfile.open(path, 'r:*') as t:
        names = {}
        for m in t.getmembers():
            problems = path_problems(m.name)
            if m.issym():
                kind = 'symlink'
            elif m.islnk():
                kind = 'hardlink'
            elif m.isdir():
                kind = 'dir'
            elif m.isfile():
                kind = 'file'
            else:
                kind = 'special'
            entry = {'name': m.name, 'size': m.size, 'kind': kind, 'mode': oct(m.mode)}
            if kind == 'file':
                fh = t.extractfile(m)
                entry['sha256'] = sha256_stream(fh) if fh else None
            members.append(entry)
            for p in problems:
                findings.append({'check': 'T1_traversal', 'member': m.name, 'detail': p})
            if kind == 'symlink':
                findings.append({'check': 'T3_symlink', 'member': m.name,
                                 'detail': 'symlink -> ' + repr(m.linkname)})
            if kind == 'hardlink':
                findings.append({'check': 'T3_hardlink', 'member': m.name,
                                 'detail': 'hardlink -> ' + repr(m.linkname)})
            if kind == 'special':
                findings.append({'check': 'T3_special', 'member': m.name,
                                 'detail': 'character/block/fifo member'})
            if m.size >= MAX_MEMBER:
                findings.append({'check': 'T4_oversize', 'member': m.name,
                                 'detail': f'{m.size} bytes >= {MAX_MEMBER}'})
            names[m.name] = names.get(m.name, 0) + 1
        for name, n in sorted(names.items()):
            if n > 1:
                findings.append({'check': 'T2_duplicate', 'member': name,
                                 'detail': f'{n} entries share this exact name'})
    return members, findings


def top_level(members, expected_root):
    tops = {}
    for m in members:
        n = m['name'].replace('\\', '/')
        while n.startswith('./'):
            n = n[2:]
        first = n.split('/', 1)[0]
        if first:
            tops[first] = tops.get(first, 0) + 1
    unexpected = sorted(k for k in tops if k != expected_root)
    return tops, unexpected


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('archive')
    ap.add_argument('--expected-root', required=True)
    ap.add_argument('--expected-sha256')
    ap.add_argument('--out')
    a = ap.parse_args()
    try:
        with open(a.archive, 'rb') as fh:
            archive_sha = sha256_stream(fh)
        size = __import__('os').path.getsize(a.archive)
        if zipfile.is_zipfile(a.archive):
            container = 'zip'
            members, findings = audit_zip(a.archive, a.expected_root)
        else:
            container = 'tar'
            members, findings = audit_tar(a.archive, a.expected_root)
    except Exception as exc:  # noqa: BLE001 - tooling failure is reported, not a verdict
        print(f'TOOLING_FAIL: {type(exc).__name__}: {exc}', file=sys.stderr)
        return 3

    tops, unexpected = top_level(members, a.expected_root)
    for k in unexpected:
        findings.append({'check': 'T5_unexpected_toplevel', 'member': k,
                         'detail': f'top-level entry outside {a.expected_root}/ ({tops[k]} members)'})

    manifest_lines = [f"{m['name']}\t{m['kind']}\t{m['size']}\t{m.get('sha256','')}"
                      for m in sorted(members, key=lambda x: x['name'])]
    member_manifest_sha = hashlib.sha256(('\n'.join(manifest_lines) + '\n').encode()).hexdigest()

    identity = {'path': a.archive, 'container': container, 'sha256': archive_sha, 'bytes': size,
                'expected_sha256': a.expected_sha256,
                'sha256_matches_expected': (a.expected_sha256 == archive_sha) if a.expected_sha256 else None,
                'expected_root': a.expected_root, 'top_level': tops,
                'member_count': len(members),
                'file_members': sum(1 for m in members if m['kind'] == 'file'),
                'symlink_members': sum(1 for m in members if m['kind'] == 'symlink'),
                'hardlink_members': sum(1 for m in members if m['kind'] == 'hardlink'),
                'max_member_bytes': max((m['size'] for m in members), default=0),
                'total_uncompressed_bytes': sum(m['size'] for m in members),
                'member_manifest_sha256': member_manifest_sha}

    report = {'schema': 1, 'identity': identity,
              'checks': {'T1_traversal': not any(f['check'] == 'T1_traversal' for f in findings),
                         'T2_duplicate': not any(f['check'] == 'T2_duplicate' for f in findings),
                         'T3_links': not any(f['check'].startswith('T3') for f in findings),
                         'T4_oversize': not any(f['check'] == 'T4_oversize' for f in findings),
                         'T5_toplevel': not unexpected,
                         'sha256_expected': identity['sha256_matches_expected']},
              'findings': findings, 'members': members}
    text = json.dumps(report, indent=2, sort_keys=True) + '\n'
    if a.out:
        with open(a.out, 'w', encoding='utf-8') as fh:
            fh.write(text)
    print(f"{a.archive}: container={container} sha256={archive_sha} bytes={size} "
          f"members={len(members)} top_level={sorted(tops)} findings={len(findings)} "
          f"member_manifest_sha256={member_manifest_sha}")
    for f in findings:
        print(f"  {f['check']}: {f['member']} -- {f['detail']}")
    return 2 if findings or identity['sha256_matches_expected'] is False else 0


if __name__ == '__main__':
    sys.exit(main())
