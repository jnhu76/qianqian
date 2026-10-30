import json
p = '/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/stage.json'
d = json.load(open(p, encoding='utf-8'))
for c in d['checks']:
    if c['id'] == 'ops/package-rc.ps1#version':
        c['detail'] = ("default -Version 0.2.0-rc.1 (:64); refuses exactly '0.1.0' (:164) and anything not matching "
                       "^[0-9]+\\.[0-9]+\\.[0-9]+([-+.0-9A-Za-z]+)?$ (:167); measured exit 1 for 0.1.0, banana, 1.2. "
                       "The immutability guard is exact-match: '0.1.0-rc.1' is NOT refused (measured exit 0, produced "
                       "songcore-v0.1.0-rc.1-windows-x86_64.zip sha256 7e843c9a68477bd575298993eca1b961b027eceba88a3793c111423d436cb264).")
new = 'evidence/songcore-172/g4/p0-entries/package-rc-version-0.1.0-rc.1.log'
if new not in d['evidence_paths']:
    d['evidence_paths'].append(new)
d['argv'].append("MSYS_NO_PATHCONV=1 powershell -NoProfile -ExecutionPolicy Bypass -File ops/package-rc.ps1 -Version 0.1.0-rc.1 -OutDir <lane>\\package-rc-out\\ver-0.1.0-rc.1")
d['exit_codes'].append(0)
json.dump(d, open(p, 'w', encoding='utf-8', newline='\n'), indent=2, ensure_ascii=False)
print('patched; argv', len(d['argv']), 'exit_codes', len(d['exit_codes']), 'paths', len(d['evidence_paths']))
