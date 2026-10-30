import hashlib
import json
import os

root = '/mnt/c/Users/fred1/source/qianqian'
stage = os.path.join(root, 'evidence/songcore-172/g4/p0-entries/stage.json')
d = json.load(open(stage, encoding='utf-8'))
hashes = []
missing = []
for rel in d['evidence_paths']:
    if rel.endswith('stage.json'):
        continue
    p = os.path.join(root, rel)
    if not os.path.exists(p):
        missing.append(rel)
        hashes.append('MISSING:' + rel)
        continue
    h = hashlib.sha256()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    hashes.append(h.hexdigest())
d['evidence_sha256'] = hashes
d['evidence_sha256_note'] = ('one hash per evidence_paths entry except stage.json itself '
                             '(self-referential, omitted); missing entries are prefixed MISSING:')
json.dump(d, open(stage, 'w', encoding='utf-8', newline='\n'), indent=2, ensure_ascii=False)
print('paths', len(d['evidence_paths']), 'hashes', len(hashes), 'missing', missing)
