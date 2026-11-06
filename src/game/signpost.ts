import * as THREE from 'three';
import type { Sfx } from './Sfx';
import { marioHitbox } from '../sim/object';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s, clamp, coss, sins, toS16, type Vec3 } from '../sim/math';
import { ACT_IDLE, ACT_FLAG_IDLE, ACT_READING_SIGN, ACT_WALKING, A_BUTTON, B_BUTTON, type ControllerSnapshot } from '../sim/state';

export interface Signpost {pos: Vec3; yaw: number; pages: string[]}

export function readableSign(signs: Signpost[], m: MarioRuntime, animation?: string): Signpost | undefined {
  if (!(m.action & ACT_FLAG_IDLE) && !(m.action === ACT_WALKING && ['anim_80','anim_7F','anim_6C'].includes(animation ?? ''))) return;
  const hitbox = marioHitbox(m);
  return signs.find(sign => {
    const [x,y,z] = sign.pos, dx = m.pos[0]-x, dz = m.pos[2]-z;
    return Math.hypot(dx,dz) < 150+hitbox.radius && m.pos[1] <= y+80 && m.pos[1]+hitbox.height >= y
      && Math.abs(toS16(atan2s(dz,dx)-sign.yaw)) <= 0x38e3
      && Math.abs(toS16(sign.yaw+0x8000-m.faceAngle[1])) <= 0x38e3;
  });
}

export class SignReading {
  private readonly box = document.createElement('div');
  private readonly text = document.createElement('div');
  private readonly eye: THREE.Vector3;
  private readonly orientation: THREE.Quaternion;
  private pitch = -0x830;
  private yaw = 0;
  private page = 0;
  private clock = 0;
  private phase: 'align' | 'open' | 'read' | 'scroll' | 'close' | 'return' = 'align';

  constructor(private sign: Signpost, m: MarioRuntime, camera: THREE.Camera, private sound: Sfx) {
    this.eye = camera.position.clone();
    this.orientation = camera.quaternion.clone();
    const [x,,z] = sign.pos;
    m.signMotion = [(x+105*sins(sign.yaw)-m.pos[0])/11, Math.trunc(toS16(sign.yaw+0x8000-m.faceAngle[1])/11),
      (z+105*coss(sign.yaw)-m.pos[2])/11];
    m.setAction(ACT_READING_SIGN);
    this.sound.signMessage('read');
    this.box.className = 'sm64-sign-dialog';
    this.box.setAttribute('role','dialog');
    this.box.setAttribute('aria-label','Sign');
    this.box.hidden = true;
    this.box.append(this.text);
    document.body.append(this.box);
  }

  dispose(): void { this.box.remove(); }

  step(m: MarioRuntime, controls: ControllerSnapshot, camera: THREE.Camera): boolean {
    if (this.phase === 'align') {
      if (m.actionState !== 2) return false;
      this.phase = 'open';
      this.sound.signMessage('open');
      this.box.hidden = false;
      this.text.textContent = this.sign.pages[0];
      this.box.style.height = (this.sign.pages[0].split('\n').length*1.4+1)+'em';
    }
    this.clock++;
    if (this.phase === 'return') {
      const factor = 1/(16-this.clock);
      camera.position.lerp(this.eye,factor);
      camera.quaternion.slerp(this.orientation,factor);
      if (this.clock === 15) {
        m.setAction(ACT_IDLE);
        this.dispose();
        return true;
      }
      return false;
    }

    this.pitch = clamp(this.pitch+Math.trunc(controls.stickY*10),-0x2000,0x38e3);
    this.yaw = clamp(this.yaw-Math.trunc(controls.stickX*10),-0x5555,0x5555);
    const yaw = m.faceAngle[1]+this.yaw+0x8000, [x,y,z] = m.pos;
    const eye = new THREE.Vector3(x+250*coss(this.pitch)*sins(yaw),y+125+250*sins(this.pitch),z+250*coss(this.pitch)*coss(yaw));
    camera.position.lerp(eye,1/Math.max(1,30-m.actionTimer));
    camera.lookAt(x,y+125,z);
    if (this.phase === 'open') {
      this.box.style.transform = 'scale('+1/Math.max(1,19-2*this.clock)+')';
      if (this.clock === 9) this.phase = 'read';
    } else if (this.phase === 'read' && controls.buttonPressed & (A_BUTTON|B_BUTTON)) {
      this.clock = 0;
      this.phase = this.page+1 === this.sign.pages.length ? 'close' : 'scroll';
      if (this.phase === 'scroll') {
        this.sound.signMessage('page');
        this.text.textContent += '\n'+this.sign.pages[this.page+1];
      }
    } else if (this.phase === 'scroll') {
      this.text.style.transform = 'translateY(-'+(1.4*this.sign.pages[this.page].split('\n').length*this.clock/8)+'em)';
      if (this.clock === 8) {
        this.text.style.transform = '';
        this.text.textContent = this.sign.pages[++this.page];
        this.phase = 'read';
      }
    } else if (this.phase === 'close') {
      if (this.clock === 3) this.sound.signMessage('close');
      this.box.style.transform = 'scale('+1/(1+2*this.clock)+')';
      if (this.clock === 9) {
        this.box.hidden = true;
        this.clock = 0;
        this.phase = 'return';
      }
    }
    return false;
  }
}
