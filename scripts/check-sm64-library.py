"""Validate exported GLBs and complete source-library coverage without dependencies."""
import json
import math
import struct
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
report=json.loads((ROOT/'assets/export-report.json').read_text())
assert not report['failed'], report['failed']
layouts=json.loads((ROOT/'assets/layouts.json').read_text())
animations=json.loads((ROOT/'assets/animations.json').read_text())
assert len(layouts)==report['sourceLayouts']
assert len(animations)==report['sourceAnimations']
paths={item['path'] for item in report['exported']}
for symbol,layout in layouts.items():
    path='assets/'+'/'.join(Path(layout['source']).parts[:2])+'/'+symbol+'.glb'
    assert path in paths, path
mesh_count=clip_count=0
for item in report['exported']:
    path=ROOT/item['path'];raw=path.read_bytes()
    assert raw[:4]==b'glTF' and struct.unpack_from('<II',raw,4)==(2,len(raw)), path
    size=struct.unpack_from('<I',raw,12)[0];doc=json.loads(raw[20:20+size]);blob=raw[28+size:]
    assert doc['asset']['extras']['sm64UnitsToMeters']==.01
    assert doc['buffers'][0]['byteLength']<=len(blob)
    for view in doc.get('bufferViews',[]):assert view.get('byteOffset',0)+view['byteLength']<=len(blob), path
    accessors=[]
    for a in doc.get('accessors',[]):
        v=doc['bufferViews'][a['bufferView']];width={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[a['type']]
        count=a['count']*width;offset=v.get('byteOffset',0)+a.get('byteOffset',0)
        assert a.get('byteOffset',0)+count*4<=v['byteLength'], path
        data=struct.unpack_from('<'+('f' if a['componentType']==5126 else 'I')*count,blob,offset)
        assert all(math.isfinite(x) for x in data),path
        accessors.append(data)
    for image in doc.get('images',[]):
        v=doc['bufferViews'][image['bufferView']];offset=v.get('byteOffset',0)
        assert blob[offset:offset+8]==b'\x89PNG\r\n\x1a\n',path
        assert all(struct.unpack_from('>II',blob,offset+16)),path
    for node in doc['nodes']:
        assert all(0<=c<len(doc['nodes']) for c in node.get('children',[])),path
    for mesh in doc.get('meshes',[]):
        mesh_count+=1
        for primitive in mesh['primitives']:
            count=doc['accessors'][primitive['attributes']['POSITION']]['count']
            assert max(accessors[primitive['indices']])<count,path
            assert len(accessors[primitive['indices']])%3==0,path
    for animation in doc.get('animations',[]):
        clip_count+=1
        for channel in animation['channels']:
            assert channel['target']['node']<len(doc['nodes']),path
            sampler=animation['samplers'][channel['sampler']]
            times=accessors[sampler['input']];assert all(a<b for a,b in zip(times,times[1:])),path
            assert doc['accessors'][sampler['input']]['count']==doc['accessors'][sampler['output']]['count'],path
            if channel['target']['path']=='rotation':
                q=accessors[sampler['output']]
                assert all(abs(sum(x*x for x in q[i:i+4])-1)<1e-5 for i in range(0,len(q),4)),path
source=json.loads((ROOT/'reference/sm64/assets.json').read_text())
expected=[p for p,data in source.items() if not p.startswith('@') and 'us' in data[-1]]
for p in expected:assert (ROOT/'assets/extracted'/p).read_bytes()==(ROOT/'reference/sm64'/p).read_bytes(),p
print(f'Validated {len(paths)} GLBs, {mesh_count} meshes, {clip_count} attached animation clips.')
print(f'Coverage: {len(layouts)} layouts, {len(animations)} source animations, {len(expected)} identical extracted resources.')
exceptions=[item for item in report['exported'] if item['issues']]
print(f'{len(exceptions)} models have documented source/rig exceptions in assets/export-report.json.')
