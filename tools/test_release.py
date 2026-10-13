#!/usr/bin/env python3
"""E01–E18 causal controls in temporary Git/ZIP fixtures, with real MinGW PE imports.
Synthetic package PASS proves policy only, never native reconstructibility.
Every subprocess verdict must match both exit code and error class/property.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

import check_release as cr

ROOT = cr.ROOT
RESULTS = []


def call(argv, cwd, expected=0, message='', env=None, label=''):
    p = subprocess.run(argv, cwd=cwd, env=cr.git_environment(env), capture_output=True, text=True)
    text = p.stdout + p.stderr
    RESULTS.append({'id': label, 'command': argv, 'cwd': str(cwd), 'exit_code': p.returncode,
                    'expected': expected, 'stdout': p.stdout, 'stderr': p.stderr})
    if p.returncode != expected or message not in text:
        raise AssertionError(f'{label}: expected {expected} {message!r}; got {p.returncode}\n{text}')
    return p.stdout.strip()


def main():
    initial_head = cr.run(['git', 'rev-parse', 'HEAD'])
    initial_status = cr.run(['git', 'status', '--porcelain', '--untracked-files=all'])
    initial_bytes = {p: p.read_bytes() for p in (ROOT / 'tools').glob('*release*') if p.is_file()}
    with tempfile.TemporaryDirectory(prefix='release-controls-') as tmp:
        repo = Path(tmp) / 'repo'
        repo.mkdir()
        for f in ('Cargo.toml', 'CHANGELOG.md', *cr.FILES[1:]):
            shutil.copyfile(ROOT / f, repo / f)
        shutil.copytree(ROOT / 'native/ffmpeg', repo / 'native/ffmpeg')
        (repo / 'native/include').mkdir()
        shutil.copyfile(ROOT / 'native/include/songcore.h', repo / 'native/include/songcore.h')
        call(['git', 'init', '-q'], repo)
        call(['git', 'add', '.'], repo)
        call(['git', '-c', 'user.name=Release Control', '-c', 'user.email=control@example.invalid',
              'commit', '-qm', 'fixture'], repo)
        sha = call(['git', 'rev-parse', 'HEAD'], repo)
        v = cr.tomllib.loads((repo / 'Cargo.toml').read_text())['workspace']['package']['version']
        tag = 'qianqian-v' + v
        common = ['--repo', str(repo), '--tag', tag, '--source', sha, '--mode', 'dry-run']
        def checker(op, tail=(), expected=0, message='', label='', args=None, env=None):
            return call([sys.executable, str(ROOT / 'tools/check_release.py'), op,
                         *(common if args is None else args), *map(str, tail)], repo,
                        expected, message, env=env, label=label)
        # Valid inputs cause each exact release-policy refusal; private changes are committed later.
        wrong = common.copy(); wrong[3] = 'qianqian-v99.99.99'
        checker('source', expected=2, message='POLICY_RED: R1 tag/workspace', label='E01', args=wrong)
        old = (repo / 'CHANGELOG.md').read_bytes()
        (repo / 'CHANGELOG.md').write_text('# no releases\n')
        call(['git', 'add', 'CHANGELOG.md'], repo)
        call(['git', '-c', 'user.name=Control', '-c', 'user.email=c@invalid', 'commit', '-qm', 'missing history'], repo)
        args = common.copy(); args[5] = call(['git', 'rev-parse', 'HEAD'], repo)
        checker('source', expected=2, message='POLICY_RED: R1 CHANGELOG', label='E02', args=args)
        call(['git', 'reset', '--hard', sha], repo)
        for bad in ('songcore-v0.1.0', 'qianqian-v01.1.0', '../bad'):
            args = common.copy(); args[3] = bad
            checker('source', expected=2, message='POLICY_RED: R1 invalid', label='E03', args=args)
        (repo / 'QUICKSTART.md').write_text('dirty')
        checker('source', expected=2, message='POLICY_RED: R2 dirty', label='E04-tracked')
        call(['git', 'add', 'QUICKSTART.md'], repo)
        checker('source', expected=2, message='POLICY_RED: R2 dirty', label='E04-staged')
        call(['git', 'reset', '--hard', sha], repo)
        (repo / 'unexpected-source').write_text('untracked')
        checker('source', expected=2, message='POLICY_RED: R2 dirty', label='E05')
        (repo / 'unexpected-source').unlink()
        checker('source', label='source-baseline')
        # Source checker must select --repo even under hook-exported Git context.
        polluted = dict(os.environ, GIT_DIR=str(ROOT / '.git'), GIT_WORK_TREE=str(ROOT),
                        GIT_INDEX_FILE=str(ROOT / '.git' / 'invalid-index'))
        checker('source', label='E18-hook-repository-isolation', env=polluted)

        # Real PE baseline; then an actual import of our controllable non-system DLL.
        pe_root = Path(tmp) / 'pe'; pe_root.mkdir()
        (pe_root / 'main.c').write_text('int main(void) { return 0; }\n')
        call(['x86_64-w64-mingw32-gcc', 'main.c', '-o', 'good.exe'], pe_root)
        (pe_root / 'dll.c').write_text('__declspec(dllexport) int foreign(void) { return 7; }\n')
        call(['x86_64-w64-mingw32-gcc', '-shared', 'dll.c', '-o', 'unshipped.dll',
              '-Wl,--out-implib,libunshipped.a'], pe_root)
        (pe_root / 'bad.c').write_text('__declspec(dllimport) int foreign(void); int main(void){return foreign();}\n')
        call(['x86_64-w64-mingw32-gcc', 'bad.c', '-L.', '-lunshipped', '-o', 'bad.exe'], pe_root)
        observed = call(['objdump', '-p', 'bad.exe'], pe_root)
        assert 'DLL Name: unshipped.dll' in observed, 'Independent E13 oracle: real PE must import DLL'
        # Explicit synthetic closure for contract tests; not a build-provenance claim.
        native = Path(tmp) / 'native'; (native / 'build/artifacts').mkdir(parents=True)
        (native / 'build/ffmpeg-xmake').mkdir()
        shutil.copytree(repo / 'native/include', native / 'include')
        call(['x86_64-w64-mingw32-gcc', '-c', 'main.c', '-o', 'sample.o'], pe_root)
        call(['x86_64-w64-mingw32-ar', 'rcs', str(native / 'build/artifacts/libsongcore.a'), 'sample.o'], pe_root)
        pin = cr.read_json(repo / 'native/ffmpeg/pin.json')
        closure = {'schema': 2, 'ffmpeg_tag': pin['ffmpeg_tag'], 'ffmpeg_commit_sha': pin['ffmpeg_commit_sha'],
                   'ffmpeg_source_sha256': pin['source_sha256'],
                   'profile_sha256': cr.file_hash(repo / 'native/ffmpeg/profiles/codec-base.json'),
                   'target': {'id': 'windows-x86_64', 'platform': 'windows', 'arch': 'x86_64',
                              'recipe_sha256': cr.file_hash(repo / 'native/ffmpeg/targets/windows-x86_64.json'),
                              'target_os': 'mingw32', 'cross_prefix': 'x86_64-w64-mingw32-'},
                   'units': [{'path': 'synthetic-contract-control.c'}]}
        cr.write_json(native / 'build/ffmpeg-xmake/manifest.json', closure)
        checker('native', ['--native', native], label='native-synthetic-baseline')
        receipt_path = native / 'build/release-provenance.json'
        receipt = cr.read_json(receipt_path)
        good_receipt = copy.deepcopy(receipt)
        receipt['archive_sha256'] = '0' * 64; cr.write_json(receipt_path, receipt)
        checker('native', ['--native', native], 2, 'POLICY_RED: R3 native archive/receipt drift', 'E14-hash')
        cr.write_json(receipt_path, good_receipt)
        archive_path = native / 'build/artifacts/libsongcore.a'
        archive_bytes = archive_path.read_bytes()
        call(['gcc', '-c', 'main.c', '-o', 'elf.o'], pe_root)
        archive_path.unlink()
        call(['ar', 'rcs', str(archive_path), 'elf.o'], pe_root)
        checker('native', ['--native', native], 2, 'POLICY_RED: R3 native archive target mismatch', 'E14-real-ELF')
        archive_path.write_bytes(archive_bytes)
        closure['target']['id'] = 'linux-x86_64'; cr.write_json(native / 'build/ffmpeg-xmake/manifest.json', closure)
        checker('native', ['--native', native], 2, 'POLICY_RED: R3 compile closure target', 'E14-target')
        closure['target']['id'] = 'windows-x86_64'; cr.write_json(native / 'build/ffmpeg-xmake/manifest.json', closure)
        name = tag + '-windows-x86_64'
        stage = Path(tmp) / name; stage.mkdir()
        for f in cr.FILES[1:]: shutil.copyfile(repo / f, stage / f)
        shutil.copyfile(pe_root / 'good.exe', stage / 'qianqian.exe')
        checker('manifest', ['--native', native, '--stage', stage], label='manifest-baseline')
        original = {f: (stage / f).read_bytes() for f in (*cr.FILES, cr.MANIFEST)}
        zip_path = Path(tmp) / (name + '.zip')
        def write_package(data, extra=(), checksum=None):
            with zipfile.ZipFile(zip_path, 'w') as z:
                for f, b in data.items(): z.writestr(name + '/' + f, b)
                for f, b in extra: z.writestr(f, b)
            Path(str(zip_path) + '.sha256').write_text((checksum or cr.file_hash(zip_path)) + '  ' + zip_path.name + '\n')
            cr.write_json(str(zip_path) + '.provenance.json', good_receipt)
        def package_case(label, data, message, extra=(), checksum=None):
            write_package(data, extra, checksum)
            checker('verify', ['--zip', zip_path], 2, 'POLICY_RED: ' + message, label)
        data = original.copy(); m = json.loads(data[cr.MANIFEST]); m['source'] = '0' * 40
        data[cr.MANIFEST] = json.dumps(m).encode(); package_case('E06', data, 'R2 manifest source')
        data = original.copy(); del data['qianqian.exe']; package_case('E07', data, 'R4 exact')
        data['qianqian-headless.exe'] = original['qianqian.exe']; package_case('E08', data, 'R4 exact')
        for f in (*cr.FILES[1:], cr.MANIFEST):
            data = original.copy(); del data[f]; package_case('E09-' + f, data, 'R4 exact')
        for f in ('../escape', name + '/extra.exe', name + '/fixtures.wav', name + '/qianqian.exe'):
            package_case('E10-' + f, original, 'R4', [(f, b'bad')])
        link = zipfile.ZipInfo(name + '/LICENSE'); link.create_system = 3; link.external_attr = 0o120777 << 16
        data = original.copy(); del data['LICENSE']; write_package(data, [(link, b'/tmp/escape')])
        checker('verify', ['--zip', zip_path], 2, 'POLICY_RED: R4 symlink', 'E10-symlink')
        data = original.copy(); data['qianqian.exe'] += b'changed'; package_case('E11', data, 'R5 payload hash')
        package_case('E12', original, 'R5 ZIP checksum', checksum='0' * 64)
        data = original.copy(); data['qianqian.exe'] = (pe_root / 'bad.exe').read_bytes()
        m = json.loads(data[cr.MANIFEST]); m['files']['qianqian.exe'] = {'sha256': cr.digest(data['qianqian.exe']), 'bytes': len(data['qianqian.exe'])}
        data[cr.MANIFEST] = json.dumps(m).encode(); package_case('E13', data, 'R4 non-system DLL import: unshipped.dll')
        write_package(original)
        missing_env = dict(os.environ, PATH='/nonexistent')
        checker('verify', ['--zip', zip_path], 3, 'TOOLING_FAIL:', 'E15-missing-tool', env=missing_env)
        zip_path.write_bytes(b'not zip'); Path(str(zip_path) + '.sha256').write_text(cr.file_hash(zip_path) + '  ' + zip_path.name + '\n')
        checker('verify', ['--zip', zip_path], 3, 'TOOLING_FAIL: BadZipFile', 'E15-zip')
        write_package(original); Path(str(zip_path) + '.provenance.json').write_text('{bad')
        checker('verify', ['--zip', zip_path], 3, 'TOOLING_FAIL: JSONDecodeError', 'E15-json')
        for event, ref, existing in [('workflow_dispatch', 'refs/tags/' + tag, False),
                                      ('push', 'refs/tags/qianqian-v0.1.0', True)]:
            checker('publish-policy', args=['--event', event, '--ref', ref, '--tag', tag] + (['--existing'] if existing else []),
                    expected=2, message='POLICY_RED: R7', label='E16')
        checker('publish-policy', args=['--event', 'push', '--ref', 'refs/tags/qianqian-v9.9.9',
                '--tag', 'qianqian-v9.9.9', '--existing'], expected=2,
                message='POLICY_RED: R7 existing', label='E16-existing-nonhistorical')
        checker('publish-policy', args=['--event', 'push', '--ref', 'refs/tags/qianqian-v9.9.9',
                '--tag', 'qianqian-v9.9.9'], label='E16-new-policy-only-no-publication')
        call(['bash', str(ROOT / 'tools/release-qianqian.sh'), 'publish'], repo, 2, 'POLICY_RED:', label='E16-local-publish')
        denied_env = dict(os.environ, GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_REF='refs/tags/' + tag)
        call([sys.executable, str(ROOT / 'tools/publish-qianqian.py'), '--tag', tag, '--source', sha, '--zip', str(zip_path)], repo, 2, 'POLICY_RED: R7', env=denied_env, label='E16-real-publisher')
        write_package(original); checker('verify', ['--zip', zip_path], label='E17')
        # A lawful committed private/comment edit must pass source policy.
        (repo / 'private.txt').write_text('private comment, no release semantics')
        call(['git', 'add', '.'], repo)
        call(['git', '-c', 'user.name=Control', '-c', 'user.email=c@invalid', 'commit', '-qm', 'private edit'], repo)
        args = common.copy(); args[5] = call(['git', 'rev-parse', 'HEAD'], repo)
        checker('source', label='E18', args=args)
    assert cr.run(['git', 'rev-parse', 'HEAD']) == initial_head
    assert cr.run(['git', 'status', '--porcelain', '--untracked-files=all']) == initial_status
    assert all(p.read_bytes() == b for p, b in initial_bytes.items())
    print('PASS: E01–E18; real PE controls; original HEAD/status/tool bytes unchanged')


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print('TOOLING_FAIL: control harness: ' + str(exc), file=sys.stderr)
        sys.exit(3)
    finally:
        if len(sys.argv) == 3 and sys.argv[1] == '--report':
            cr.write_json(sys.argv[2], {'head': cr.run(['git', 'rev-parse', 'HEAD']), 'executions': RESULTS})
