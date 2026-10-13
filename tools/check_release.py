#!/usr/bin/env python3
"""Shared release contract. No compilation or external writes in verify/source."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'x86_64-pc-windows-gnu'
FILES = ('qianqian.exe', 'QUICKSTART.md', 'LICENSE', 'LICENSE-MIT',
         'LICENSE-APACHE', 'THIRD_PARTY_NOTICES.md')
MANIFEST = 'BUILD-MANIFEST.txt'
# Existing package-windows system allowlist, shared by build and offline verify.
SYSTEM_DLLS = set(('kernel32 user32 userenv ws2_32 advapi32 shell32 bcrypt '
                  'bcryptprimitives combase msvcrt ntdll ole32 oleaut32 propsys '
                  'rpcrt4 gdi32 imm32 uxtheme dwmapi shcore secur32 crypt32 '
                  'sspicli version winmm msacm32').split())


class PolicyError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise PolicyError(message)


def run(argv, cwd=ROOT):
    return subprocess.run(argv, cwd=cwd, check=True, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout.strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    return digest(Path(path).read_bytes())


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')


def version(tag):
    match = re.fullmatch(r'qianqian-v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))', tag)
    require(match is not None, 'R1 invalid product tag (SongCore tags are separate)')
    return match[1]


def source(args):
    v = version(args.tag)
    require(re.fullmatch(r'[0-9a-f]{40}', args.source) is not None, 'R2 expected full source SHA')
    require(run(['git', 'rev-parse', 'HEAD'], args.repo) == args.source, 'R2 HEAD != expected source')
    require(not run(['git', 'status', '--porcelain', '--untracked-files=all'], args.repo), 'R2 dirty source')
    if args.mode == 'formal':
        # Missing tag is a policy failure; Git executable failures remain tooling failures.
        refs = run(['git', 'tag', '--list', args.tag], args.repo).splitlines()
        require(args.tag in refs, 'R1 formal tag missing')
        require(run(['git', 'rev-parse', args.tag + '^{commit}'], args.repo) == args.source,
                'R2 peeled tag != source')
    cargo = tomllib.loads((args.repo / 'Cargo.toml').read_text())
    require(cargo['workspace']['package']['version'] == v, 'R1 tag/workspace version mismatch')
    require((args.repo / 'CHANGELOG.md').is_file(), 'R1 CHANGELOG file missing')
    require(re.search(r'^## ' + re.escape(v) + r' — \d{4}-\d{2}-\d{2}$',
                      (args.repo / 'CHANGELOG.md').read_text(), re.M), 'R1 CHANGELOG release missing')
    return v


def imports(exe):
    out = run(['objdump', '-p', str(exe)])
    require('file format pei-x86-64' in out, 'R4 canonical executable must be Windows x86_64 PE')
    dlls = sorted(set(re.findall(r'DLL Name:\s*(\S+)', out)), key=str.lower)
    require(dlls, 'R4 PE has no imports')
    for dll in dlls:
        low = dll.lower()
        require(low.endswith('.dll') and (low[:-4] in SYSTEM_DLLS or
                low.startswith(('api-ms-win-', 'ext-ms-win-'))),
                'R4 non-system DLL import: ' + dll)
    return dlls


def native_identity(repo, native, source_sha):
    closure = read_json(native / 'build/ffmpeg-xmake/manifest.json')
    pin = read_json(repo / 'native/ffmpeg/pin.json')
    recipe = read_json(repo / 'native/ffmpeg/targets/windows-x86_64.json')
    archive = native / 'build/artifacts/libsongcore.a'
    # objdump inspects every real archive member, not a text fixture.
    out = run(['objdump', '-f', str(archive)])
    formats = re.findall(r'file format (\S+)', out)
    require(formats and all(f == 'pe-x86-64' for f in formats), 'R3 native archive target mismatch')
    require(file_hash(native / 'include/songcore.h') == file_hash(repo / 'native/include/songcore.h'),
            'R3 native header mismatch')
    receipt = {
        'schema': 1, 'source': source_sha, 'target': TARGET,
        'archive_sha256': file_hash(archive),
        'header_sha256': file_hash(native / 'include/songcore.h'),
        'pin_sha256': file_hash(repo / 'native/ffmpeg/pin.json'),
        'recipe_sha256': file_hash(repo / 'native/ffmpeg/targets/windows-x86_64.json'),
        'profile_sha256': file_hash(repo / 'native/ffmpeg/profiles/codec-base.json'),
        'closure_sha256': digest(json.dumps(closure, sort_keys=True).encode()),
        'closure': closure,
        'tools': {name: run(cmd).splitlines()[0] for name, cmd in {
            'rustc': ['rustc', '--version'], 'mingw': ['x86_64-w64-mingw32-gcc', '--version'],
            'xmake': ['xmake', '--version']}.items()},
    }
    check_receipt(repo, receipt, source_sha)
    # Ensure the freshly generated closure binds the actual current recipes/pin.
    require(recipe['id'] == 'windows-x86_64' and pin['ffmpeg_tag'] == closure['ffmpeg_tag'], 'R3 pin identity')
    return receipt


def check_receipt(repo, receipt, source_sha):
    require(receipt['schema'] == 1 and receipt['source'] == source_sha and receipt['target'] == TARGET,
            'R3 native receipt source/target mismatch')
    for key, rel in [('header', 'include/songcore.h'), ('pin', 'ffmpeg/pin.json'),
                     ('recipe', 'ffmpeg/targets/windows-x86_64.json'),
                     ('profile', 'ffmpeg/profiles/codec-base.json')]:
        require(receipt[key + '_sha256'] == file_hash(repo / 'native' / rel), 'R3 ' + key + ' hash mismatch')
    require(re.fullmatch('[0-9a-f]{64}', receipt['archive_sha256']), 'R3 archive hash missing')
    require(all(isinstance(receipt['tools'][t], str) and receipt['tools'][t].strip()
                for t in ('rustc', 'mingw', 'xmake')), 'R3 tool identity missing')
    c = receipt['closure']
    require(receipt['closure_sha256'] == digest(json.dumps(c, sort_keys=True).encode()), 'R3 closure hash mismatch')
    pin = read_json(repo / 'native/ffmpeg/pin.json')
    require(c['schema'] == 2 and all(c[k] == pin[p] for k, p in [
        ('ffmpeg_tag', 'ffmpeg_tag'), ('ffmpeg_commit_sha', 'ffmpeg_commit_sha'),
        ('ffmpeg_source_sha256', 'source_sha256')]), 'R3 FFmpeg pin mismatch')
    require(c['profile_sha256'] == receipt['profile_sha256'] and c['units'], 'R3 profile/closure missing')
    require(c['target'] == {'id': 'windows-x86_64', 'platform': 'windows', 'arch': 'x86_64',
        'recipe_sha256': receipt['recipe_sha256'], 'target_os': 'mingw32',
        'cross_prefix': 'x86_64-w64-mingw32-'}, 'R3 compile closure target mismatch')


def manifest(args):
    v = source(args)
    receipt = read_json(args.native / 'build/release-provenance.json')
    check_receipt(args.repo, receipt, args.source)
    require(file_hash(args.native / 'build/artifacts/libsongcore.a') == receipt['archive_sha256'],
            'R3 native archive hash mismatch')
    dlls = imports(args.stage / 'qianqian.exe')
    data = {'schema': 2, 'package': f'{args.tag}-windows-x86_64', 'version': v,
            'tag': args.tag, 'source': args.source, 'source_status': 'clean', 'target': TARGET,
            'mode': args.mode, 'built_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'native': receipt, 'dll_imports': dlls,
            'files': {f: {'sha256': file_hash(args.stage / f), 'bytes': (args.stage / f).stat().st_size}
                      for f in FILES},
            'reproducibility': 'RECIPE_RECONSTRUCTIBLE; BYTE_REPRODUCIBLE not established',
            'windows_audio': 'NOT_RUN'}
    write_json(args.stage / MANIFEST, data)


def verify(args):
    v = source(args)
    name = f'{args.tag}-windows-x86_64'
    require(args.zip.name == name + '.zip', 'R5 ZIP filename mismatch')
    checksum = Path(str(args.zip) + '.sha256').read_text()
    require(checksum == file_hash(args.zip) + '  ' + args.zip.name + '\n', 'R5 ZIP checksum mismatch')
    receipt = read_json(str(args.zip) + '.provenance.json')
    check_receipt(args.repo, receipt, args.source)
    with zipfile.ZipFile(args.zip) as z:
        infos = z.infolist()
        names = [i.filename for i in infos]
        allowed = {name + '/' + f for f in (*FILES, MANIFEST)}
        require(len(names) == len(set(names)), 'R4 duplicate ZIP paths')
        require(set(names) in (allowed, allowed | {name + '/'}), 'R4 exact package file set mismatch')
        for i in infos:
            require(not stat.S_ISLNK(i.external_attr >> 16), 'R4 symlink ZIP member')
            require(i.file_size < 100_000_000, 'R4 unexpected member size')
        data = {f: z.read(name + '/' + f) for f in (*FILES, MANIFEST)}
    m = json.loads(data[MANIFEST])
    require(m['schema'] == 2 and m['package'] == name and m['version'] == v and m['tag'] == args.tag,
            'R1 package identity mismatch')
    require(m['source'] == args.source and m['source_status'] == 'clean', 'R2 manifest source mismatch')
    require(m['target'] == TARGET and m['mode'] == args.mode, 'R2 mode/target mismatch')
    require(m['native'] == receipt, 'R3 external native receipt mismatch')
    require(set(m['files']) == set(FILES), 'R5 manifest exact payload set (no self-hash)')
    for f in FILES:
        require(m['files'][f] == {'sha256': digest(data[f]), 'bytes': len(data[f])}, 'R5 payload hash/size mismatch: ' + f)
        if f != 'qianqian.exe':
            expected = subprocess.run(['git', 'show', args.source + ':' + f], cwd=args.repo,
                                      check=True, stdout=subprocess.PIPE).stdout
            require(data[f] == expected, 'R2 shipped documentation differs from source: ' + f)
    with tempfile.TemporaryDirectory(prefix='qianqian-verify-') as tmp:
        exe = Path(tmp) / 'qianqian.exe'
        exe.write_bytes(data['qianqian.exe'])
        require(imports(exe) == m['dll_imports'], 'R4 recorded DLL imports differ from real PE')


def publication_policy(event, ref, tag, existing):
    version(tag)
    require(event == 'push' and ref == 'refs/tags/' + tag, 'R7 publication requires actual tag push')
    require(tag != 'qianqian-v0.1.0' and not existing, 'R7 existing/historical release cannot be replaced')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    for op in ('source', 'native', 'manifest', 'verify', 'notes'):
        p = sub.add_parser(op)
        p.add_argument('--repo', type=Path, default=ROOT)
        p.add_argument('--tag', required=True)
        p.add_argument('--source', required=True)
        p.add_argument('--mode', choices=('formal', 'dry-run'), required=True)
        if op in ('native', 'manifest'):
            p.add_argument('--native', type=Path, required=True)
        if op == 'manifest':
            p.add_argument('--stage', type=Path, required=True)
        if op in ('verify', 'notes'):
            p.add_argument('--zip', type=Path, required=True)
    p = sub.add_parser('publish-policy')
    p.add_argument('--event', required=True)
    p.add_argument('--ref', required=True)
    p.add_argument('--tag', required=True)
    p.add_argument('--existing', action='store_true')
    args = parser.parse_args()
    try:
        if args.operation == 'source':
            source(args)
        elif args.operation == 'native':
            source(args)
            current = native_identity(args.repo, args.native, args.source)
            receipt_path = args.native / 'build/release-provenance.json'
            if receipt_path.exists():
                require(current == read_json(receipt_path), 'R3 native archive/receipt drift')
            else:
                write_json(receipt_path, current)
        elif args.operation == 'manifest':
            manifest(args)
        elif args.operation == 'verify':
            verify(args)
        elif args.operation == 'notes':
            verify(args)
            text = (args.repo / 'CHANGELOG.md').read_text()
            section = re.split(r'^## ', text, flags=re.M)
            print(next(s for s in section if s.startswith(version(args.tag) + ' — ')).strip())
            print('\nDistribution: Windows x86_64 portable ZIP\n\nSHA-256: `' + file_hash(args.zip) + '`')
            print('\nSource: `' + args.source + '`; see attached provenance JSON.\n'
                  'WASAPI device/audio: NOT_RUN by this build; byte reproducibility not established.')
            return
        else:
            publication_policy(args.event, args.ref, args.tag, args.existing)
        print('PASS: ' + args.operation)
    except PolicyError as exc:
        print('POLICY_RED: ' + str(exc), file=sys.stderr)
        sys.exit(2)
    except Exception as exc:
        print('TOOLING_FAIL: ' + type(exc).__name__ + ': ' + str(exc), file=sys.stderr)
        sys.exit(3)


if __name__ == '__main__':
    main()
