"""Export decomp GeoLayouts, Fast3D display lists and rigid animations to GLB.
Uses only Python's standard library; run extract_assets.py us first.
"""
import argparse
import ast
import collections
import functools
import json
import math
import re
import shutil
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
SCALE = .01


def bake_decal(data, color=None, start_row=0):
    """N64 BLENDRGB: opaque texels replace shade; alpha-zero texels retain it."""
    width,height,depth,kind=struct.unpack_from('>IIBB',data,16)
    if depth!=8 or kind not in (0,2,4,6):raise ValueError(f'Unsupported PNG for decal baking: depth={depth}, kind={kind}')
    channels={0:1,2:3,4:2,6:4}[kind]; chunks=[];offset=8
    while offset<len(data):
        size=struct.unpack_from('>I',data,offset)[0];tag=data[offset+4:offset+8]
        if tag==b'IDAT':chunks.append(data[offset+8:offset+8+size])
        offset+=size+12
    raw=zlib.decompress(b''.join(chunks));stride=width*channels;previous=bytearray(stride);out=bytearray();pos=0
    for y in range(height):
        mode=raw[pos];pos+=1;row=bytearray(raw[pos:pos+stride]);pos+=stride
        for i in range(stride):
            a=row[i-channels] if i>=channels else 0;b=previous[i];c=previous[i-channels] if i>=channels else 0
            if mode==1:pred=a
            elif mode==2:pred=b
            elif mode==3:pred=(a+b)//2
            elif mode==4:
                p=a+b-c;dist=[abs(p-a),abs(p-b),abs(p-c)];pred=[a,b,c][dist.index(min(dist))]
            elif mode==0:pred=0
            else:raise ValueError('Unsupported PNG filter')
            row[i]=(row[i]+pred)&255
        if y>=start_row:
            out.append(0)
            for x in range(width):
                i=x*channels;alpha=row[i+channels-1]/255 if kind in (4,6) else 1
                rgb=[row[i]]*3 if kind in (0,4) else row[i:i+3]
                out.extend(round(rgb[k]*alpha+color[k]*255*(1-alpha)) if color is not None else rgb[k] for k in range(3));out.append(255 if color is not None else round(alpha*255))
        previous=row
    def chunk(tag,payload):return struct.pack('>I',len(payload))+tag+payload+struct.pack('>I',zlib.crc32(tag+payload)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height-start_row,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(out))+chunk(b'IEND',b'')


def clean(s):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', s, flags=re.S)


def us_source(text):
    """Select US preprocessor branches without expanding display-list macros."""
    stack=[];active=True;out=[]
    def condition(expression):
        expression=re.sub(r'defined\s*\(?\s*(\w+)\s*\)?',lambda m:'1' if m[1] in ('VERSION_US','TARGET_N64') else '0',expression)
        expression=re.sub(r'\b[A-Za-z_]\w*\b',lambda m:'1' if m[0] in ('VERSION_US','TARGET_N64') else '0',expression)
        expression=expression.replace('&&',' and ').replace('||',' or ');expression=re.sub(r'!(?!=)',' not ',expression)
        tree=ast.parse(expression.strip(),mode='eval')
        if any(not isinstance(n,(ast.Expression,ast.BoolOp,ast.UnaryOp,ast.Constant,ast.And,ast.Or,ast.Not)) for n in ast.walk(tree)):
            raise ValueError('Unsupported preprocessor condition '+expression)
        return bool(eval(compile(tree,'<condition>','eval'),{'__builtins__':{}}))
    for line in text.splitlines():
        m=re.match(r'\s*#(ifdef|ifndef|if|elif|else|endif)\b(.*)',line)
        if not m:
            if active:out.append(line)
            continue
        directive,arg=m[1],m[2].strip()
        if directive in ('if','ifdef','ifndef'):
            yes=condition(arg if directive=='if' else f'defined({arg})')
            if directive=='ifndef':yes=not yes
            stack.append([active,yes]);active=active and yes
        elif directive in ('else','elif'):
            parent,seen=stack[-1];yes=not seen and (directive=='else' or condition(arg));stack[-1][1]|=yes;active=parent and yes
        else:active=stack.pop()[0]
    return '\n'.join(out)


def split(s):
    return [v.strip() for v in re.split(r',(?![^()]*\))', s) if v.strip()]


def num(s):
    s = s.strip()
    constants = {'NULL': 0, 'TRUE': 1, 'FALSE': 0}
    if s in constants:
        return constants[s]
    node = ast.parse(s, mode='eval').body
    def visit(n):
        if isinstance(n, ast.Constant) and isinstance(n.value, (int, float)): return n.value
        if isinstance(n, ast.UnaryOp) and isinstance(n.op, (ast.USub, ast.UAdd)):
            return -visit(n.operand) if isinstance(n.op, ast.USub) else visit(n.operand)
        if isinstance(n, ast.BinOp):
            a,b=visit(n.left),visit(n.right)
            if isinstance(n.op, ast.Add): return a+b
            if isinstance(n.op, ast.Sub): return a-b
            if isinstance(n.op, ast.Mult): return a*b
            if isinstance(n.op, ast.Div): return a/b
            if isinstance(n.op, ast.LShift): return int(a)<<int(b)
            if isinstance(n.op, ast.BitOr): return int(a)|int(b)
        raise ValueError(f'Unsupported numeric expression: {s}')
    return visit(node)


@functools.lru_cache(None)
def commands(body):
    result=[]
    for m in re.finditer(r'\b((?:gs|GEO_)\w+)\s*\(',body):
        start=m.end(); depth=1; end=start
        while depth and end<len(body):
            depth += (body[end]=='(')-(body[end]==')'); end+=1
        result.append((m[1],split(body[start:end-1])))
    return result


def quat(x,y,z):
    # SM64 mtxf_rotate_xyz_and_translate uses Rz * Ry * Rx on column vectors.
    sx,cx=math.sin(x/2),math.cos(x/2)
    sy,cy=math.sin(y/2),math.cos(y/2)
    sz,cz=math.sin(z/2),math.cos(z/2)
    return [sx*cy*cz-cx*sy*sz,cx*sy*cz+sx*cy*sz,cx*cy*sz-sx*sy*cz,cx*cy*cz+sx*sy*sz]


class Source:
    def __init__(self):
        self.arrays={}; self.geos={}; self.lights={}; self.animations={}; self.textures={}
        for folder in ['actors','levels','textures','bin','assets/anims']:
            for path in sorted((REF/folder).rglob('*.c')):
                text=us_source(clean(path.read_text()))
                for m in re.finditer(r'\b(?:const\s+)?(?:GeoLayout|Gfx|Vtx|Texture|u8|u16|s16|u64)\s+(\w+)\s*\[[^\]]*\]\s*=\s*\{(.*?)\};',text,re.S):
                    name,body=m[1],m[2]
                    self.arrays[name]=(body,path)
                    if 'GeoLayout' in m[0].split('{')[0]: self.geos[name]=(body,path)
                    offset=0
                    for inc in re.finditer(r'#include\s+"([^"]+)\.inc\.c"',body):
                        image=REF/(inc[1]+'.png')
                        if not image.exists():image=ROOT/'assets/extracted'/(inc[1]+'.png')
                        if not image.exists():continue
                        self.textures[name if offset==0 else f'{name}+{offset}']=image
                        w,h=struct.unpack_from('>II',image.read_bytes(),16)
                        bits=re.search(r'(?:rgba|ia|i|ci)(4|8|16|32)$',inc[1]);offset+=w*h*int(bits[1] if bits else 16)//8
                for m in re.finditer(r'Lights1\s+(\w+)\s*=\s*gdSPDefLights1\((.*?)\)',text,re.S):
                    self.lights[m[1]]=[num(x)/255 for x in split(m[2])[3:6]]
                for m in re.finditer(r'struct Animation\s+(\w+)(?:\[\])?\s*=\s*\{(.*?)\};',text,re.S):
                    self.animations[m[1]]=(split(m[2]),path)

        # MODEL_CAP_SWITCH_BASE is loaded directly from a display list.
        self.geos['castle_lobby_light'] = ('GEO_DISPLAY_LIST(LAYER_TRANSPARENT,dl_castle_lobby_wing_cap_light), GEO_END()', REF/'levels/castle_inside/areas/1/geo.inc.c')
        self.geos['cap_switch_base'] = ('GEO_DISPLAY_LIST(LAYER_OPAQUE,cap_switch_base_seg5_dl_05003120), GEO_END()', REF/'actors/capswitch/geo.inc.c')

    @functools.lru_cache(None)
    def vertices(self,name):
        body=self.arrays[name][0]
        rows=[]
        for match in re.finditer(r'\{\{(.*?)\}\}',body,re.S):
            vals=[num(x) for x in re.findall(r'-?0x[\da-fA-F]+|-?\d+',match[1])]
            if len(vals)!=10: raise ValueError(f'Unexpected vertex in {name}: {vals}')
            rows.append(vals)
        return rows

    @functools.lru_cache(None)
    def ints(self,name):
        return [int(num(x)) for x in split(self.arrays[name][0])]


class Export:
    def __init__(self,source,symbol,choices=None):
        self.s=source; self.symbol=symbol; self.bin=bytearray(); self.meshcache={}; self.texcache={}; self.matcache={}
        self.choices=choices or {}
        self.doc={'asset':{'version':'2.0','generator':'Jumpman decomp exporter','extras':{'sm64UnitsToMeters':SCALE,'symbol':symbol}},'scene':0,'scenes':[{'nodes':[0]}], 'nodes':[], 'meshes':[], 'materials':[], 'textures':[], 'images':[], 'samplers':[], 'bufferViews':[], 'accessors':[], 'animations':[], 'extensionsUsed':['KHR_materials_unlit']}
        self.issues=collections.Counter(); self.joints=[]
        self.state={'texture':None,'enabled':False,'light':(1,1,1),'lighting':True,'cullBack':True,'cullFront':False,'size':(32,32),'uvscale':(1,1),'wrap':(10497,10497),'cache':{},'combine':''}

    def view(self,data):
        self.bin.extend(b'\0'*((-len(self.bin))%4)); offset=len(self.bin); self.bin.extend(data)
        i=len(self.doc['bufferViews']); self.doc['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(data)})
        return i

    def accessor(self,rows,kind,integer=False):
        size={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[kind]
        flat=[v for row in rows for v in (row if isinstance(row,(list,tuple)) else [row])]
        view=self.view(struct.pack('<'+('I' if integer else 'f')*len(flat),*flat))
        d={'bufferView':view,'componentType':5125 if integer else 5126,'count':len(flat)//size,'type':kind}
        d['min']=[min(flat[i::size]) for i in range(size)]; d['max']=[max(flat[i::size]) for i in range(size)]
        i=len(self.doc['accessors']);self.doc['accessors'].append(d);return i

    def material(self,layer):
        st=self.state; tex=st['texture'] if st['enabled'] else None
        blend='BLENDRGB' in st['combine']
        tk=(tex,tuple(st['wrap']),tuple(st['light']) if blend else None)
        key=(tex,tuple(st['light']),tuple(st['wrap']),layer,st['lighting'],st['combine'],st['cullBack'],st['cullFront'])
        if key in self.matcache:
            if tex and tk in self.texcache:
                _,w,h=self.texcache[tk];st['size']=(w,h)
            return self.matcache[key]
        # glTF factors are linear; source RGB and PNG pixels use the same display space.
        color=[c/12.92 if c<=0.04045 else ((c+0.055)/1.055)**2.4 for c in st['light']] if st['lighting'] else [1,1,1]
        if tex and (blend or 'DECALRGB' in st['combine']):color=[1,1,1]
        mat={'name':str(tex or color),'pbrMetallicRoughness':{'baseColorFactor':color+[1],'metallicFactor':0,'roughnessFactor':1},'doubleSided':not (st['cullBack'] or st['cullFront'])}
        if not st['lighting']:mat['extensions']={'KHR_materials_unlit':{}}
        if tex:
            path=self.s.textures.get(tex)
            byte_offset=0
            if path is None and '+' in tex:
                base,offset=tex.rsplit('+',1);path=self.s.textures.get(base);byte_offset=int(offset)
            if path:
                if tk not in self.texcache:
                    data=path.read_bytes(); w,h=struct.unpack_from('>II',data,16)
                    if byte_offset:
                        bits=re.search(r'(?:rgba|ia|i|ci)(4|8|16|32)\.png$',path.name)
                        rowbytes=w*int(bits[1] if bits else 16)//8
                        if byte_offset%rowbytes:raise ValueError('Texture offset is not row aligned')
                        data=bake_decal(data,start_row=byte_offset//rowbytes);w,h=struct.unpack_from('>II',data,16)
                    if blend:data=bake_decal(data,st['light'])
                    image=len(self.doc['images']); self.doc['images'].append({'bufferView':self.view(data),'mimeType':'image/png','name':path.name})
                    sampler=len(self.doc['samplers']); self.doc['samplers'].append({'magFilter':9728,'minFilter':9728,'wrapS':st['wrap'][0],'wrapT':st['wrap'][1]})
                    index=len(self.doc['textures']);self.doc['textures'].append({'source':image,'sampler':sampler});self.texcache[tk]=(index,w,h)
                index,w,h=self.texcache[tk];st['size']=(w,h)
                mat['pbrMetallicRoughness']['baseColorTexture']={'index':index}
            else:self.issues['missing texture '+str(tex)]+=1
        if 'ALPHA' in layer:mat.update(alphaMode='MASK',alphaCutoff=.5)
        elif 'TRANSPARENT' in layer:mat['alphaMode']='BLEND'
        idx=len(self.doc['materials']);self.doc['materials'].append(mat);self.matcache[key]=idx;return idx

    def display(self,name,layer):
        primitives=collections.defaultdict(list)
        def run(symbol,depth=0):
            if depth>60:raise ValueError('Recursive display list '+symbol)
            if symbol not in self.s.arrays:
                if symbol!='NULL':self.issues['missing display '+symbol]+=1
                return
            st=self.state
            for cmd,a in commands(self.s.arrays[symbol][0]):
                if cmd in ('gsSPDisplayList','gsSPBranchList'):
                    run(a[0],depth+1)
                    if cmd=='gsSPBranchList':break
                elif cmd=='gsSPVertex':
                    match=re.match(r'&?(\w+)(?:\s*\+\s*(\d+)|\[(\d+)\])?',a[0])
                    vn=match[1];offset=int(match[2] or match[3] or 0)
                    verts=self.s.vertices(vn)
                    for i in range(int(num(a[1]))):
                        # Some original lists DMA beyond the declared array but only
                        # reference valid vertices (e.g. Hoot's six-vertex wing).
                        if offset+i>=len(verts):continue
                        st['cache'][int(num(a[2]))+i]=verts[offset+i]
                elif cmd in ('gsSP1Triangle','gsSP2Triangles'):
                    indices=[int(num(a[i])) for i in ([0,1,2] if cmd=='gsSP1Triangle' else [0,1,2,4,5,6])]
                    if st['cullBack'] and st['cullFront']:continue
                    if st['cullFront']:
                        for i in range(0,len(indices),3):indices[i+1],indices[i+2]=indices[i+2],indices[i+1]
                    mat=self.material(layer); w,h=st['size']
                    for i in indices:
                        v=st['cache'][i];normal=[(n if n<128 else n-256)/127 for n in v[6:9]]
                        color=[c/255 for c in v[6:10]] if not st['lighting'] else [1,1,1,1]
                        primitives[mat].append(([x*SCALE for x in v[:3]],normal,[v[4]*st['uvscale'][0]/(32*w),v[5]*st['uvscale'][1]/(32*h)],color))
                elif cmd in ('gsDPSetTextureImage','gsDPLoadTextureBlock','gsDPLoadTextureBlock_4b'):
                    st['texture']=a[-1] if cmd=='gsDPSetTextureImage' else a[0]
                    expression=re.sub(r'\([^)]*\*\)','',st['texture']).replace(' ','')
                    if '+' in expression:
                        base,offset=expression.split('+',1);expression=f'{base}+{int(num(offset))}'
                    st['texture']=expression
                    if cmd!='gsDPSetTextureImage':
                        start=6 if cmd=='gsDPLoadTextureBlock' else 5
                        st['wrap']=tuple(33071 if 'CLAMP' in s else 33648 if 'MIRROR' in s and 'NOMIRROR' not in s else 10497 for s in a[start:start+2])
                elif cmd=='gsSPTexture':
                    st['enabled']=a[-1] not in ('G_OFF','0')
                    st['uvscale']=tuple(num(x)/65536 for x in a[:2])
                elif cmd=='gsDPSetCombineMode':st['combine']=a[0]
                elif cmd=='gsSPLight' and a[1]=='1':
                    ln=re.sub(r'^&|\.[la]$', '',a[0]);st['light']=self.s.lights.get(ln,(1,1,1))
                elif cmd in ('gsSPSetGeometryMode','gsSPClearGeometryMode','gsSPGeometryMode','gsSPGeometryModeSetFirst'):
                    modes = [(a[0],cmd=='gsSPSetGeometryMode')]
                    if cmd=='gsSPGeometryMode':modes=[(a[0],False),(a[1],True)]
                    if cmd=='gsSPGeometryModeSetFirst':modes=[(a[0],True),(a[1],False)]
                    for flags,enabled in modes:
                        if 'G_LIGHTING' in flags:st['lighting']=enabled
                        if 'G_CULL_BACK' in flags or 'G_CULL_BOTH' in flags:st['cullBack']=enabled
                        if 'G_CULL_FRONT' in flags or 'G_CULL_BOTH' in flags:st['cullFront']=enabled
                elif cmd=='gsDPSetTile' and len(a)>10 and a[4]=='G_TX_RENDERTILE':
                    st['wrap']=tuple(33071 if 'CLAMP' in s else 33648 if 'MIRROR' in s and 'NOMIRROR' not in s else 10497 for s in [a[9],a[6]])
        run(name)
        parts=[]
        for mat,verts in primitives.items():
            if not verts:continue
            pos,norm,uv,color=zip(*verts)
            parts.append({'attributes':{'POSITION':self.accessor(pos,'VEC3'),'NORMAL':self.accessor(norm,'VEC3'),'TEXCOORD_0':self.accessor(uv,'VEC2'),'COLOR_0':self.accessor(color,'VEC4')},'indices':self.accessor(list(range(len(pos))),'SCALAR',True),'material':mat})
        if not parts:return None
        i=len(self.doc['meshes']);self.doc['meshes'].append({'name':name,'primitives':parts});return i

    def node(self,cmd,args,parent):
        idx=len(self.doc['nodes']);n={'name':f'{cmd}_{idx}','extras':{'sm64Command':cmd,'sm64Args':args}}
        self.doc['nodes'].append(n)
        if parent is not None:self.doc['nodes'][parent].setdefault('children',[]).append(idx)
        dl=None;layer=args[0] if args else 'LAYER_OPAQUE'
        if cmd=='GEO_SCALE':n['scale']=[num(args[1])/65536]*3
        elif cmd=='GEO_ANIMATED_PART':
            n['translation']=[num(x)*SCALE for x in args[1:4]];dl=args[4];self.joints.append(idx)
        elif cmd=='GEO_DISPLAY_LIST':dl=args[1]
        elif cmd.startswith('GEO_TRANSLATE_ROTATE'):
            n['translation']=[num(x)*SCALE for x in args[1:4]];n['rotation']=quat(*[math.radians(num(x)) for x in args[4:7]])
            if cmd.endswith('_WITH_DL'):dl=args[7]
        elif cmd.startswith('GEO_TRANSLATE') or cmd.startswith('GEO_BILLBOARD_WITH_PARAMS'):
            n['translation']=[num(x)*SCALE for x in args[1:4]]
            if cmd.endswith('_WITH_DL'):dl=args[4]
        elif cmd.startswith('GEO_ROTATION_NODE'):
            n['rotation']=quat(*[math.radians(num(x)) for x in args[1:4]])
            if cmd.endswith('_WITH_DL'):dl=args[4]
        if dl and dl!='NULL':
            mesh=self.display(dl,layer)
            if mesh is not None:n['mesh']=mesh
        return idx

    def geo(self,symbol,parent=None,depth=0):
        if symbol=='NULL':return
        if depth>40:raise ValueError('Recursive geo '+symbol)
        if symbol not in self.s.geos:self.issues['missing geo '+symbol]+=1;return
        stack=[parent]; last=parent; switch_counts={}; switch_states={}; skip_depth=0
        for cmd,a in commands(self.s.geos[symbol][0]):
            if skip_depth:
                if cmd=='GEO_OPEN_NODE':skip_depth+=1
                elif cmd=='GEO_CLOSE_NODE':skip_depth-=1
                continue
            if cmd=='GEO_OPEN_NODE':stack.append(last);continue
            if cmd=='GEO_CLOSE_NODE':
                # Closing restores the node that was opened, not its parent.
                # Goomba reopens the same animated node for its head and feet.
                if len(stack)>1:last=stack.pop()
                continue
            if cmd in ('GEO_END','GEO_RETURN'):break
            p=stack[-1]
            if p is not None and self.doc['nodes'][p]['extras']['sm64Command']=='GEO_SWITCH_CASE':
                # Switch alternatives execute independently, never in sequence.
                # Otherwise blinking bakes the previous head's brown hair light into the cap logo.
                if p not in switch_states:switch_states[p]={**self.state,'cache':dict(self.state['cache'])}
                self.state={**switch_states[p],'cache':dict(switch_states[p]['cache'])}
                pn=self.doc['nodes'][p];count=switch_counts.get(p,0);switch_counts[p]=count+1
                # Keep eye alternatives for the runtime callback. Other switches use case 0.
                callback=pn['extras']['sm64Args'][-1]
                selected=self.choices.get(callback,0)
                if count!=selected and not (callback=='geo_switch_mario_eyes' and callback not in self.choices):
                    last=self.node('GEO_NODE_START',[],p);self.doc['nodes'][last]['scale']=[0,0,0]
                    # Skip children of this inactive alternative while keeping stack balanced.
                    # The placeholder is stripped after layout traversal.
                    self.doc['nodes'][last]['extras']['inactive']=True
                    continue
            if p is not None and self.doc['nodes'][p]['extras'].get('inactive'):
                last=self.node('GEO_NODE_START',[],p);self.doc['nodes'][last]['extras']['inactive']=True;continue
            if cmd in ('GEO_BRANCH','GEO_BRANCH_AND_LINK'):
                last=self.node(cmd,a,p);self.geo(a[-1],last,depth+1)
            elif cmd=='GEO_RENDER_RANGE' and (num(a[0])!=self.choices['_lod_min'] if '_lod_min' in self.choices else num(a[0])>0):
                last=self.node('GEO_NODE_START',[],p)
                self.doc['nodes'][last]['extras']['inactive']=True
            else:last=self.node(cmd,a,p)

    def animate(self):
        if not self.joints:return
        srcpath=self.s.geos[self.symbol][1]
        actor=srcpath.relative_to(REF).parts[:2]
        root=self.joints[0];rootbase=list(self.doc['nodes'][root].get('translation',[0,0,0]));firstpose=None
        for name,(a,path) in self.s.animations.items():
            is_mario=actor==('actors','mario') and path.is_relative_to(REF/'assets/anims')
            if not is_mario and not path.is_relative_to(REF.joinpath(*actor)/'anims'):continue
            flags,divisor,start,loopstart,frames=[int(num(x)) for x in a[:5]]
            values=[v if v<32768 else v-65536 for v in self.s.ints(a[6])];indices=self.s.ints(a[7])
            count=len(indices)//6-1
            if is_mario and len(self.joints)!=20:
                continue  # Helper layouts are partial rigs; clips live on full rigs.
            if count<len(self.joints):
                self.issues[f'animation joint count {name}: {count} != {len(self.joints)}']+=1
            times=self.accessor([f/30 for f in range(frames)],'SCALAR')
            animation={'name':name,'samplers':[],'channels':[],'extras':{'fps':30,'startFrame':start,'loopStart':loopstart,'loop':not bool(flags&1),'backward':bool(flags&2),'flags':flags}}
            def sample(component,frame):
                n,offset=indices[component*2:component*2+2]
                return values[offset+min(frame,n-1)]
            def channel(node,path,data,kind):
                sampler=len(animation['samplers']);animation['samplers'].append({'input':times,'output':self.accessor(data,kind),'interpolation':'STEP'})
                animation['channels'].append({'sampler':sampler,'target':{'node':node,'path':path}})
            base=rootbase; multiplier=189/divisor if divisor and is_mario else 1
            translation=[]
            for f in range(frames):
                tr=[sample(i,f)*SCALE*multiplier for i in range(3)]
                if flags&8:tr[0]=tr[2]=0
                elif flags&16:tr[1]=0
                elif flags&64:tr=[0,0,0]
                translation.append([base[i]+tr[i] for i in range(3)])
            channel(root,'translation',translation,'VEC3')
            if firstpose is None:firstpose=translation[0]
            for j,node in enumerate(self.joints[:count]):
                rotations=[quat(*[sample(3+j*3+i,f)*math.tau/65536 for i in range(3)]) for f in range(frames)]
                channel(node,'rotation',rotations,'VEC4')
            self.doc['animations'].append(animation)
        # SM64 adds an animation's root translation to the animated part's own translation, and a
        # door object always has its closed clip running, so the rest pose of a door is that clip's
        # first frame (the hinge sits 150 units left of the part origin, centring the 154-unit
        # leaf on the object).  Without this, doors that never get a clip started (every hinged
        # room door at load) are drawn 75 units off-centre and leave a gap beside the frame.
        if actor==('actors','door') and firstpose is not None:self.doc['nodes'][root]['translation']=firstpose

    def save(self,path):
        self.geo(self.symbol)
        self.animate()
        self.doc['asset']['extras']['geometryless']=not bool(self.doc['meshes'])
        if not self.bin:self.bin.extend(b'\0'*4)
        self.doc['buffers']=[{'byteLength':len(self.bin)}]
        for key in list(self.doc):
            if self.doc[key]==[]:del self.doc[key]
        raw=json.dumps(self.doc,separators=(',',':')).encode();raw+=b' '*((-len(raw))%4)
        self.bin.extend(b'\0'*((-len(self.bin))%4))
        data=struct.pack('<III',0x46546c67,2,28+len(raw)+len(self.bin))+struct.pack('<II',len(raw),0x4e4f534a)+raw+struct.pack('<II',len(self.bin),0x004e4942)+self.bin
        path.parent.mkdir(parents=True,exist_ok=True)
        if not path.exists() or path.read_bytes()!=data:path.write_bytes(data)
        return {'path':str(path.relative_to(ROOT)), 'meshes':len(self.doc.get('meshes',[])),'joints':len(self.joints),'animations':len(self.doc.get('animations',[])),'issues':dict(self.issues)}


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--all',action='store_true',help='Export the complete library (the default)');ap.add_argument('--required-only',action='store_true',help='Export only models referenced by the current game');args=ap.parse_args()
    source=Source();required={'actors/mario/mario_geo.glb','actors/coin/yellow_coin_no_shadow_geo.glb','actors/star/star_geo.glb','actors/goomba/goomba_geo.glb','actors/tree/bubbly_tree_geo.glb'}
    for path in (ROOT/'assets/courses').glob('*.json'):
        data=json.loads(path.read_text())  # triggers/spawners and .ext.json overlays carry no model
        required.update(m for m in [data.get('model')]+[o.get('model') for o in data.get('objects',[])] if m)
    if not args.required_only:
        for symbol,(_,path) in source.geos.items():
            parts=path.relative_to(REF).parts
            required.add('/'.join(parts[:2])+'/'+symbol+'.glb')
    # Switch and LOD variants below use the parent layout with an explicit selection.
    required={rel for rel in required if "/variants/" not in rel}
    report=[];failed=[]
    for rel in sorted(required):
        symbol=Path(rel).stem
        try:
            result=Export(source,symbol).save(ROOT/'assets'/rel);report.append(result)
            print(f'{rel}: {result["meshes"]} meshes, {result["animations"]} animations, {len(result["issues"])} issue types',flush=True)
        except Exception as e:
            failed.append({'path':rel,'error':str(e)});print(f'FAILED {rel}: {e}',flush=True)
    # Export switch alternatives separately so unused expressions, hands, caps,
    # coin frames and effect frames remain available without overlapping meshes.
    if not args.required_only:
        for symbol,(body,path) in source.geos.items():
            cmds=commands(body);cases={}
            for i,(cmd,a) in enumerate(cmds):
                if cmd!='GEO_SWITCH_CASE':continue
                depth=0;count=0
                for child,_ in cmds[i+1:]:
                    if child=='GEO_OPEN_NODE':depth+=1
                    elif child=='GEO_CLOSE_NODE':
                        depth-=1
                        if depth<=0:break
                    elif depth==1:count+=1
                cases[a[-1]]=max(cases.get(a[-1],0),count)
            for callback,count in cases.items():
                for case in range(1,count):
                    rel='/'.join(path.relative_to(REF).parts[:2])+f'/variants/{symbol}/{callback}_{case}.glb'
                    try:
                        result=Export(source,symbol,{callback:case}).save(ROOT/'assets'/rel)
                        result['selection']={callback:case};report.append(result)
                    except Exception as e:failed.append({'path':rel,'error':str(e)})
            for minimum in sorted({int(num(a[0])) for cmd,a in cmds if cmd=='GEO_RENDER_RANGE' and num(a[0])>0}):
                rel='/'.join(path.relative_to(REF).parts[:2])+f'/variants/{symbol}/lod_{minimum}.glb'
                try:
                    result=Export(source,symbol,{'_lod_min':minimum}).save(ROOT/'assets'/rel)
                    result['selection']={'lodMinimum':minimum};report.append(result)
                except Exception as e:failed.append({'path':rel,'error':str(e)})
        # Preserve every extracted ROM resource and every animation, including
        # clips whose intended skeleton is a runtime-selected layout.
        extracted=[]
        for rel,info in json.loads((REF/'assets.json').read_text()).items():
            if rel.startswith('@') or 'us' not in info[-1]:continue
            dest=ROOT/'assets/extracted'/rel;dest.parent.mkdir(parents=True,exist_ok=True)
            if (REF/rel).is_file():shutil.copyfile(REF/rel,dest)
            elif not dest.is_file():raise FileNotFoundError(rel)
            extracted.append(rel)
        animations={}
        for name,(a,path) in source.animations.items():
            animations[name]={'source':str(path.relative_to(REF)),'header':a[:6],'values':source.ints(a[6]),'indices':source.ints(a[7])}
        (ROOT/'assets/animations.json').write_text(json.dumps(animations,separators=(',',':')))
        layouts={name:{'source':str(path.relative_to(REF)),'commands':commands(body)} for name,(body,path) in source.geos.items()}
        (ROOT/'assets/layouts.json').write_text(json.dumps(layouts,separators=(',',':')))
    else:extracted=[];animations={};layouts={}
    (ROOT/'assets/export-report.json').write_text(json.dumps({'exported':report,'failed':failed,'sourceLayouts':len(layouts),'sourceAnimations':len(animations),'extractedResources':len(extracted)},indent=2))
    print(f'Exported {len(report)}; failed {len(failed)}')
    if failed:raise SystemExit(1)

if __name__=='__main__':main()
