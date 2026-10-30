import json
p = '/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/stage.json'
d = json.load(open(p, encoding='utf-8'))
new = 'evidence/songcore-172/g4/p0-entries/entry-inventory.txt'
if new not in d['evidence_paths']:
    d['evidence_paths'].append(new)
json.dump(d, open(p, 'w', encoding='utf-8', newline='\n'), indent=2, ensure_ascii=False)
print('paths', len(d['evidence_paths']))
