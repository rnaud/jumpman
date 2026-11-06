import widths from '../../assets/oot-dialogue/widths.json';

export interface MessagePage {text: string; style: number; branches?: MessagePage[][]}
const sources = import.meta.glob<string>('../../assets/oot-dialogue/*.png',{eager:true,query:'?url',import:'default'});
const colors = ['#fff','#ff3c3c','#46ff50','#505aff','#64b4ff','#ff96b4','#e1ff32','#000'];
const woodColors = ['#fff','#ff7800','#46ff50','#506eff','#5ab4ff','#d264ff','#ffff1e','#000'];
const controls = ['A','B','C','L','R','Z','C-Up','C-Down','C-Left','C-Right','target','Control Stick'];
const textures = new Map<string,HTMLImageElement>();
const painted = new Map<string,HTMLCanvasElement>();

function art(name: string,color: string) {
  let img = textures.get(name);
  if (!img) {
    img = new Image();
    img.src = sources[`../../assets/oot-dialogue/${name}.png`];
    textures.set(name,img);
  }
  if (!img.complete || !img.naturalWidth) return;
  const key = name+color;
  let canvas = painted.get(key);
  if (!canvas) {
    canvas=document.createElement('canvas');
    canvas.width=img.width; canvas.height=img.height;
    const ctx=canvas.getContext('2d')!;
    ctx.drawImage(img,0,0);
    ctx.globalCompositeOperation = name==='wood' ? 'multiply' : 'source-in';
    ctx.fillStyle=color; ctx.fillRect(0,0,canvas.width,canvas.height);
    if (name==='wood') {
      ctx.globalCompositeOperation='destination-in';
      ctx.drawImage(img,0,0);
    }
    painted.set(key,canvas);
  }
  return canvas;
}

export class OotTextbox {
  readonly box = document.getElementById('kokiri-dialogue')!;
  private readonly canvas = this.box.querySelector('canvas')!;
  private readonly ctx = this.canvas.getContext('2d')!;
  private readonly accessible = this.box.querySelector('p')!;
  private page?: MessagePage;
  private pageStarted = 0;
  private opened = 0;
  private visibleCount = 0;
  private hasNext = false;
  choice = 0;
  private stickHeld = false;

  show(speaker:string, page:MessagePage, hasNext:boolean, immediate=false) {
    this.page = page;
    this.hasNext = hasNext;
    this.choice = 0;
    this.stickHeld = false;
    this.pageStarted = performance.now();
    if (this.box.hidden) this.opened = this.pageStarted;
    this.visibleCount = immediate ? Infinity : 0;
    this.box.hidden = false;
    this.box.querySelector('strong')!.textContent = speaker;
    this.accessible.textContent = page.text.replace(/[\x05\x06][\s\S]/g,'').replace(/[\x08\x09\x1b\x1c]/g,'')
      .replace(/[\x9f-\xaa]/g,c=>'['+controls[c.charCodeAt(0)-159]+']');
  }

  hide() {this.page=undefined; this.box.hidden=true;}

  /** A first tap completes an animating page; the next follows the original continuation. */
  finishReveal() {
    if (!this.page || this.visibleCount>=this.page.text.length) return false;
    this.visibleCount=Infinity;
    return true;
  }

  choose(stickY:number) {
    const count=this.page?.branches?.length ?? 0;
    if (!count || Math.abs(stickY)<30) {this.stickHeld=false;return;}
    if (!this.stickHeld) this.choice=(this.choice+count+(stickY>0?-1:1))%count;
    this.stickHeld=true;
  }

  draw(view: HTMLCanvasElement, actorY:number) {
    const page=this.page;
    if (!page) return;
    const rect=view.getBoundingClientRect();
    const scale=Math.min(rect.width/320,rect.height/240);
    const position=page.style & 15;
    const top=position===1 ? 38 : position===2 ? 90 : position===3 ? 142 : actorY>0.59 ? 38 : 142;
    const box=this.box.style;
    const frame = Math.floor((performance.now()-this.opened)*0.02);
    const grow = [0.6,0.75,0.9,1,1.05,1.1,1.05,1][frame] ?? 1;
    box.transform = grow === 1 ? '' : `scale(${grow})`;
    box.width=256*scale+'px';
    box.left=rect.left+rect.width/2-126*scale+'px';
    box.top=rect.top+rect.height/2+(top-120)*scale+'px';
    const ctx=this.ctx;
    ctx.clearRect(0,0,256,64);
    const wood=page.style>>4===1;
    const background=art(wood?'wood':'black',wood?'#654429':'#000');
    if (background) {
      ctx.globalAlpha=wood?230/255:170/255;
      ctx.drawImage(background,0,0);
      ctx.save();ctx.translate(256,0);ctx.scale(-1,1);ctx.drawImage(background,0,0);ctx.restore();
      ctx.globalAlpha=1;
    }
    const text=page.text;
    const elapsed=performance.now()-this.pageStarted;
    this.visibleCount=Math.max(this.visibleCount,Math.floor(elapsed/21)+1);
    let limit=this.visibleCount;
    let x=31,y=({1:26,2:20,3:16}[Math.min(4,text.split('\n').length)] ?? 8);
    let color=0,quick=false,choiceY=0,choiceBlock=false;
    for (let i=0;i<text.length;i++) {
      const code=text.charCodeAt(i);
      if (code===8) {if(i>=limit) break;quick=true;continue;}
      if (code===9) {quick=false;limit=Math.max(limit,i+1);continue;}
      if (i>=limit && !quick) break;
      if (code===5) {color=(text.charCodeAt(++i)-64)&7;continue;}
      if (code===6) {x+=text.charCodeAt(++i);continue;}
      if (code===27||code===28) {choiceBlock=true;choiceY=y;x+=16;continue;}
      if (code===10) {x=choiceBlock?47:31;y+=12;continue;}
      const idx=code-32;
      if (idx<0||idx>=widths.length) continue;
      const col=(wood?woodColors:colors)[color];
      const glyphs=art('glyphs',col),shadow=art('glyphs','#000');
      const sx=idx%16*16,sy=Math.floor(idx/16)*16;
      if (glyphs && shadow) {
        ctx.drawImage(shadow,sx,sy,16,16,x+1,y+1,12,12);
        ctx.drawImage(glyphs,sx,sy,16,16,x,y,12,12);
      }
      x+=Math.floor(widths[idx]*.75);
    }
    if (quick || limit>=text.length) this.visibleCount=Infinity;
    if (this.visibleCount>=text.length) {
      const icon=art(choiceBlock?'arrow':this.hasNext?'next':'end','#65afff');
      if (icon) ctx.drawImage(icon,choiceBlock?31:121,choiceBlock?choiceY+this.choice*12:49,12,12);
    }
  }
}
