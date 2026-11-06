import * as THREE from 'three';
import { A_BUTTON, B_BUTTON, type ControllerSnapshot } from '../sim/state';
import { COURSES, SCORE_COURSES, missionsFor, type CourseId } from './courses';
import type { SaveFile } from './save';

export function actChoices(collected: readonly boolean[]) {
  const next = collected.indexOf(false);
  const firstOpen = next < 0 ? 6 : next+1;
  const lastCollected = collected.lastIndexOf(true)+1;
  const visible = Math.max(firstOpen,lastCollected);
  const selectable = Array.from({length:visible},(_,i)=>i+1).filter(act => collected[act-1] || act===firstOpen);
  return {firstOpen,visible,selectable};
}

const hudTextures = import.meta.glob<string>('../../assets/extracted/textures/segment2/segment2.0{00,02,04,06,08,0A,0C,0E,10,12,56,58}00.rgba16.png',
  {eager:true, query:'?url', import:'default'});
function hudSymbol(value: number): HTMLImageElement {
  const offset = (value*0x200).toString(16).toUpperCase().padStart(5,'0');
  const icon = new Image();
  icon.src = hudTextures[`../../assets/extracted/textures/segment2/segment2.${offset}.rgba16.png`];
  icon.alt = '';
  return icon;
}

// Eight-pixel US menu font from leveldata.c. Characters advance by their dialog widths.
const menuGlyphs = import.meta.glob<string>('../../assets/extracted/levels/menu/main_menu_seg7_us.*.ia8.png',
  {eager:true, query:'?url', import:'default'});
const widths = [7,7,7,7,7,7,7,7,7,7,6,6,6,6,6,6,6,6,5,6,6,5,8,8,6,6,6,6,6,5,6,6,8,7,6,6];
const punctuation: Record<string,number> = {'-':0xb600,',':0xb640,"'":0xb680,'’':0xb680,'!':0xb6c0,'?':0xb700,'.':0xb7c0,'&':0xb800};
function menuText(element: HTMLElement, text: string): void {
  const accessible = document.createElement('span');
  accessible.className = 'act-readable';
  accessible.textContent = text;
  element.replaceChildren(accessible);
  for (const char of text.toUpperCase()) {
    const index = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ'.indexOf(char);
    const addr = index >= 0 ? 0xac40+index*64 : punctuation[char];
    const glyph = document.createElement('span');
    glyph.setAttribute('aria-hidden','true');
    glyph.className = 'act-glyph';
    glyph.style.width = (index >= 0 ? widths[index] : 4)/8+'em';
    if (addr) {
      const img = new Image();
      img.alt = '';
      img.src = menuGlyphs[`../../assets/extracted/levels/menu/main_menu_seg7_us.${addr.toString(16).toUpperCase().padStart(5,'0')}.ia8.png`];
      glyph.append(img);
    } else if(char!==' ') glyph.innerText = char;
    element.append(glyph);
  }
}

/** star_select.c uses six mission flags. The 100-coin star is a separate badge. */
export class ActSelector {
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(45,4/3,100,25000);
  private readonly root = document.querySelector<HTMLElement>('#act-selector')!;
  private readonly name = this.root.querySelector<HTMLElement>('.act-name')!;
  private readonly row = this.root.querySelector<HTMLElement>('.act-stars')!;
  private readonly stars: THREE.Group[] = [];
  private readonly buttons: HTMLButtonElement[] = [];
  private readonly choices: ReturnType<typeof actChoices>;
  private selection: number;
  private timer = 0;
  private holdTimer = 0;
  private lastDirection = 0;

  constructor(readonly course: CourseId, save: SaveFile, models: THREE.Group,
      private readonly accept: (act: number) => void, private readonly moveSound: () => void) {
    const collected = Array.from({length:6},(_,i)=>save.hasStar(course,i+1));
    this.choices = actChoices(collected);
    this.selection = collected.every(Boolean) ? 1 : this.choices.firstOpen;
    this.scene.background = new THREE.Color(0xffffff);
    this.camera.position.z = 1000;
    this.scene.add(new THREE.AmbientLight(0xffffff,1.7));
    const light = new THREE.DirectionalLight(0xffffff,2);
    light.position.set(-1,2,3);
    this.scene.add(light);
    this.row.replaceChildren();
    const addStar = (gold: boolean,x: number,y: number) => {
      const star = new THREE.Group();
      const model = models.children[gold ? 0 : 1].clone();
      model.visible = true;
      star.add(model);
      star.position.set(x,y,-300);
      this.scene.add(star);
      this.stars.push(star);
      return star;
    };
    for(let i=0;i<this.choices.visible;i++) {
      addStar(collected[i],(this.choices.visible-1)*-75+i*152,248);
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = String(i+1);
      button.disabled = !this.choices.selectable.includes(i+1);
      button.setAttribute('aria-label',missionsFor(course)[i].name);
      button.addEventListener('click',()=>{
        if (this.timer<=10) return;
        this.select(i+1);
        this.confirm();
      });
      this.buttons.push(button);
      this.row.append(button);
    }
    if(save.hasStar(course,7)) addStar(true,370,24).scale.setScalar(0.8);
    this.root.querySelector('.act-course')!.textContent = COURSES[course].name;
    const number = String([...SCORE_COURSES].indexOf(course)+1);
    const badge = this.root.querySelector<HTMLElement>('.act-number')!;
    badge.setAttribute('aria-label','Course '+number);
    badge.replaceChildren(...Array.from(number,n=>hudSymbol(Number(n))));
    const score = this.root.querySelector<HTMLElement>('.act-score')!;
    score.hidden = save.bestCoins(course)===0;
    const record = String(save.bestCoins(course));
    score.setAttribute('aria-label','My Score: '+record+' coins');
    score.replaceChildren('My Score　',hudSymbol(0x2c),hudSymbol(0x2b),...Array.from(record,n=>hudSymbol(Number(n))));
    this.updateLabel();
    this.root.hidden = false;
    document.body.classList.add('choosing-act');
  }

  private select(act: number): void {
    if (act===this.selection) return;
    this.selection = act;
    this.moveSound();
    this.updateLabel();
  }

  private updateLabel(): void {
    menuText(this.name,missionsFor(this.course)[this.selection-1].name);
    this.buttons.forEach((button,i)=>button.setAttribute('aria-current',String(i+1===this.selection)));
  }

  confirm(): void {
    if (this.timer>10) this.accept(Math.min(this.choices.firstOpen,this.selection));
  }

  step(input: ControllerSnapshot): void {
    const direction = Math.abs(input.stickX)>45 ? Math.sign(input.stickX) : 0;
    if(direction && direction!==this.lastDirection) {
      const acts = this.choices.selectable;
      const next = acts.indexOf(this.selection)+direction;
      if(next>=0 && next<acts.length) this.select(acts[next]);
    }
    if(this.holdTimer===10) { this.holdTimer=8; this.lastDirection=0; }
    else { this.holdTimer++; this.lastDirection=direction; }
    if(!direction) this.holdTimer=0;
    this.stars.forEach((star,i)=>{
      if(i===this.choices.visible) { star.rotation.y+=Math.PI/16; return; }
      const active = i+1===this.selection;
      star.scale.setScalar(active ? Math.min(1.3,star.scale.x+0.1) : Math.max(1,star.scale.x-0.1));
      star.rotation.y = active ? star.rotation.y+Math.PI/16 : 0;
    });
    if(input.buttonPressed & (A_BUTTON|B_BUTTON)) this.confirm();
    this.timer++;
  }

  render(renderer: THREE.WebGLRenderer): void {
    const canvas = renderer.domElement;
    const aspect = canvas.clientWidth/Math.max(1,canvas.clientHeight);
    if(aspect!==this.camera.aspect) { this.camera.aspect=aspect; this.camera.updateProjectionMatrix(); }
    this.root.style.width=canvas.clientWidth+'px';
    this.root.style.height=canvas.clientHeight+'px';
    this.camera.updateMatrixWorld();
    this.buttons.forEach((button,i)=>{
      const point=this.stars[i].position.clone().project(this.camera);
      button.style.left=(point.x+1)*50+'%';
    });
    renderer.render(this.scene,this.camera);
  }

  close(): void {
    this.root.hidden = true;
    this.row.replaceChildren();
    document.body.classList.remove('choosing-act');
  }
}
