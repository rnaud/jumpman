import type { MarioRuntime } from '../sim/MarioRuntime';
import { actionName } from '../sim/state';

function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing #${id}`);
  return element as T;
}

export interface HudState {
  coins: number;
  totalCoins?: number;
  time: number;
  fps: number;
  zone: string;
  starReady: boolean;
  stars: number;
  totalStars: number;
  interaction?: string | null;
}

export class HUD {
  private readonly action = byId('action-name');
  private readonly speedBar = byId('speed-bar');
  private readonly speedLabel = byId('speed-label');
  private readonly jumpCombo = byId('jump-combo');
  private readonly healthSegments = Array.from(document.querySelectorAll<HTMLElement>('#health-track i'));
  private readonly starCount = byId('star-count');
  private readonly coinCount = byId('coin-count');
  private readonly timeValue = byId('time-value');
  private readonly fpsValue = byId('fps-value');
  private readonly zone = byId('zone');
  private readonly debug = byId('debug');
  private readonly ready = byId('star-ready');
  private readonly toast = byId('toast');
  private readonly interaction = byId('interaction-prompt');

  private toastTimer: number | null = null;
  debugVisible = false;

  toggleDebug(): void {
    this.debugVisible = !this.debugVisible;
    this.debug.hidden = !this.debugVisible;
  }

  showToast(text: string, duration = 2200): void {
    this.toast.textContent = text;
    this.toast.classList.add('visible');
    if (this.toastTimer !== null) window.clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => {
      this.toast.classList.remove('visible');
      this.toastTimer = null;
    }, duration);
  }

  showDialogPage(text: string | undefined): void {
    if (this.toastTimer !== null) window.clearTimeout(this.toastTimer);
    this.toastTimer = null;
    this.toast.textContent = text ?? '';
    this.toast.classList.toggle('visible', !!text);
  }

  update(m: MarioRuntime, state: HudState): void {
    this.action.textContent = actionName(m.action);
    const speedRatio = Math.min(1, Math.abs(m.forwardVel) / 48);
    this.speedBar.style.width = `${Math.round(speedRatio * 100)}%`;
    this.speedLabel.textContent = `${m.forwardVel.toFixed(1)} u/f`;
    this.jumpCombo.textContent = m.jumpCombo > 0 ? '★'.repeat(m.jumpCombo) : '';

    const health = Math.max(0, Math.min(this.healthSegments.length, Math.ceil(m.health / 0x110)));
    this.healthSegments.forEach((segment, index) => {
      segment.classList.toggle('empty', index >= health);
    });

    this.starCount.textContent = `${state.stars} / ${state.totalStars}`;
    this.coinCount.textContent = state.totalCoins === undefined ? String(state.coins) : `${state.coins} / ${state.totalCoins}`;
    this.timeValue.textContent = formatTime(state.time);
    this.fpsValue.textContent = `${Math.round(state.fps)}`;
    this.zone.textContent = state.zone;
    this.ready.hidden = !state.starReady;

    const prompt = state.interaction ?? '';
    this.interaction.textContent = prompt;
    this.interaction.classList.toggle('visible', prompt.length > 0);

    if (this.debugVisible) {
      this.renderDebug(m);
    }
  }

  private renderDebug(m: MarioRuntime): void {
    const floor = m.floor;
    const wall = m.wall;
    const floorNormal = floor ? floor.normal.map((v) => v.toFixed(2)).join(', ') : '—';
    const wallNormal = wall ? wall.normal.map((v) => v.toFixed(2)).join(', ') : '—';
    const inputs: string[] = [];
    if (m.input & 0x0001) inputs.push('STICK');
    if (m.input & 0x0002) inputs.push('A!');
    if (m.input & 0x0080) inputs.push('A');
    if (m.input & 0x2000) inputs.push('B!');
    if (m.input & 0x4000) inputs.push('Z');
    if (m.input & 0x8000) inputs.push('Z!');
    if (m.input & 0x0004) inputs.push('AIR');
    if (m.input & 0x0008) inputs.push('SLIDE');
    if (m.input & 0x0020) inputs.push('IDLE-IN');

    this.debug.innerHTML = [
      `<span class="warn">ACTION</span> 0x${m.action.toString(16)}  ${actionName(m.action)}`,
      `pos      ${m.pos.map((v) => v.toFixed(1)).join('  ')}`,
      `vel      ${m.vel.map((v) => v.toFixed(2)).join('  ')}`,
      `forward  ${m.forwardVel.toFixed(3)}   yaw ${m.faceAngle[1]}`,
      `floor    h=${m.floorHeight.toFixed(1)}  n=[${floorNormal}]`,
      `         angle=${m.floorAngle}  steep=${m.floorIsSteep()}  slip=${m.floorIsSlippery()}`,
      `ceil     h=${m.ceilHeight.toFixed(1)}`,
      `wall     n=[${wallNormal}]`,
      `peak     ${m.peakHeight.toFixed(1)}   quicksand ${m.quicksandDepth.toFixed(1)}`,
      `timers   dj=${m.doubleJumpTimer} wk=${m.wallKickTimer} squish=${m.squishTimer}`,
      `input    ${inputs.join(' ') || '—'}`,
      `camera   yaw=${m.cameraYaw}  stick=${m.controller.stickX.toFixed(0)},${m.controller.stickY.toFixed(0)}`,
      `health   0x${m.health.toString(16)}  respawns=${m.respawns}`,
      `held     ${m.heldObj ? `${m.heldObj.behavior} #${m.heldObj.id}` : '—'}`,
    ].join('\n');
  }
}

export function formatTime(seconds: number): string {
  const clamped = Math.max(0, seconds);
  const minutes = Math.floor(clamped / 60);
  const secs = Math.floor(clamped % 60);
  const tenths = Math.floor((clamped * 10) % 10);
  return `${minutes}:${secs.toString().padStart(2, '0')}.${tenths}`;
}
