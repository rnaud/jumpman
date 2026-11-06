import { A_BUTTON, B_BUTTON, Z_TRIG } from '../sim/state';

const actions: Record<string, number> = { A: A_BUTTON, B: B_BUTTON, Z: Z_TRIG };

export class TouchControls {
  stick = { x: 0, y: 0, mag: 0 };
  buttons = 0;
  pressed = 0;
  focus = false;
  private readonly owners = new Map<HTMLElement, number>();
  private readonly pad: HTMLElement;

  constructor(root: HTMLElement, onInput: () => void) {
    this.pad = root.querySelector<HTMLElement>('.touch-stick')!;
    for (const control of root.querySelectorAll<HTMLElement>('[data-touch]')) {
      const action = control.dataset.touch!;
      const mask = actions[action] ?? 0;
      control.addEventListener('pointerdown', event => {
        event.preventDefault();
        if (this.owners.has(control)) return;
        control.setPointerCapture(event.pointerId);
        this.owners.set(control, event.pointerId);
        control.classList.add('held');
        this.buttons |= mask;
        this.pressed |= mask;
        if (action === 'focus') this.focus = true;
        if (control === this.pad) this.move(event);
        onInput();
      });
      control.addEventListener('pointermove', event => {
        if (control === this.pad && this.owners.get(control) === event.pointerId) this.move(event);
      });
      const release = (event: PointerEvent) => {
        if (this.owners.get(control) !== event.pointerId) return;
        this.owners.delete(control);
        control.classList.remove('held');
        this.buttons &= ~mask;
        if (action === 'focus') this.focus = false;
        if (control === this.pad) this.setStick(0, 0);
      };
      for (const type of ['pointerup', 'pointercancel', 'lostpointercapture'] as const) {
        control.addEventListener(type, release);
      }
    }
  }

  private move(event: PointerEvent): void {
    const rect = this.pad.getBoundingClientRect();
    const radius = rect.width / 2;
    this.setStick((event.clientX - rect.left - radius) / radius, (rect.top + radius - event.clientY) / radius);
  }

  private setStick(x: number, y: number): void {
    const length = Math.max(1, Math.hypot(x, y));
    x /= length;
    y /= length;
    this.stick = { x: x * 64, y: y * 64, mag: Math.hypot(x, y) * 64 };
    this.pad.style.setProperty('--stick-x', x * 32 + 'px');
    this.pad.style.setProperty('--stick-y', -y * 32 + 'px');
  }

  clear(): void {
    for (const element of this.owners.keys()) element.classList.remove('held');
    this.owners.clear();
    this.buttons = this.pressed = 0;
    this.focus = false;
    this.setStick(0, 0);
  }
}
