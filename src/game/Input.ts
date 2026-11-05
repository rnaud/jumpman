/**
 * Keyboard, mouse and gamepad input.  Button edges are latched until the next
 * 30 Hz physics step, so a quick tap can never fall between frames.
 */

import {
  A_BUTTON,
  B_BUTTON,
  Z_TRIG,
  type ControllerSnapshot,
} from '../sim/state';

const MOVE_KEYS = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD']);
export interface PointerHandler {
  down(event: PointerEvent): boolean;
  move(event: PointerEvent): boolean;
  up(): void;
  wheel(delta: number): boolean;
}

export interface CameraInput {
  dx: number;
  dy: number;
  wheel: number;
  orbitX: number;
  orbitY: number;
}

export class InputManager {
  private readonly keys = new Set<string>();
  private readonly justPressed = new Set<string>();

  private pendingA = false;
  private pendingB = false;
  private pendingZ = false;

  private mouseDX = 0;
  private mouseDY = 0;
  private wheelDelta = 0;

  private dragging = false;
  private lastPointerX = 0;
  private lastPointerY = 0;

  private gamepadIndex: number | null = null;
  private startPressed = false;
  private padButtons: boolean[] = [];
  private padAxes: number[] = [0, 0, 0, 0];

  anyInput = false;
  private pointerHandler: PointerHandler | null = null;
  gamepadConnected = false;

  constructor(private readonly canvas: HTMLCanvasElement) {
    window.addEventListener('keydown', this.onKeyDown);
    window.addEventListener('keyup', this.onKeyUp);
    window.addEventListener('blur', this.onBlur);

    canvas.addEventListener('pointerdown', this.onPointerDown);
    window.addEventListener('pointermove', this.onPointerMove);
    window.addEventListener('pointerup', this.onPointerUp);
    canvas.addEventListener('wheel', this.onWheel, { passive: false });
    canvas.addEventListener('contextmenu', (event) => event.preventDefault());

    window.addEventListener('gamepadconnected', (event) => {
      this.gamepadIndex = (event as GamepadEvent).gamepad.index;
      this.gamepadConnected = true;
    });
    window.addEventListener('gamepaddisconnected', () => {
      this.padButtons = [];
      this.padAxes = [0, 0, 0, 0];
      this.startPressed = false;
      this.gamepadIndex = null;
      this.gamepadConnected = false;
    });
  }

  private onKeyDown = (event: KeyboardEvent): void => {
    if (event.target instanceof HTMLElement && event.target.closest('dialog, button, select, input, a')) return;
    if (event.code === 'Space' || event.code.startsWith('Arrow') || event.code === 'Tab') {
      event.preventDefault();
    }
    if (event.code === 'F3' || event.code === 'Backquote' || event.code === 'KeyG') {
      event.preventDefault();
    }

    this.anyInput = true;
    if (!this.keys.has(event.code) && !event.repeat) {
      this.justPressed.add(event.code);
      if (event.code === 'Space') this.pendingA = true;
      if (event.code === 'ShiftLeft' || event.code === 'ShiftRight' || event.code === 'KeyJ') {
        this.pendingB = true;
      }
      if (
        event.code === 'ControlLeft' ||
        event.code === 'ControlRight' ||
        event.code === 'KeyK'
      ) {
        this.pendingZ = true;
      }
    }
    this.keys.add(event.code);
  };

  private onKeyUp = (event: KeyboardEvent): void => {
    this.keys.delete(event.code);
  };

  clear(): void {
    this.keys.clear(); this.justPressed.clear(); this.dragging = false;
    this.pendingA = this.pendingB = this.pendingZ = false;
    this.mouseDX = this.mouseDY = this.wheelDelta = 0;
  }
  private onBlur = (): void => { this.clear(); };

  setPointerHandler(handler: PointerHandler | null): void {
    this.pointerHandler = handler;
  }

  private onPointerDown = (event: PointerEvent): void => {
    if (this.pointerHandler?.down(event)) {
      this.dragging = false;
      return;
    }
    if (event.button !== 0 && event.button !== 2) return;
    this.anyInput = true;
    this.dragging = true;
    this.lastPointerX = event.clientX;
    this.lastPointerY = event.clientY;
    this.canvas.setPointerCapture(event.pointerId);
    this.canvas.style.cursor = 'grabbing';
  };

  private onPointerMove = (event: PointerEvent): void => {
    if (this.pointerHandler?.move(event)) return;
    if (!this.dragging) return;
    const dx = event.clientX - this.lastPointerX;
    const dy = event.clientY - this.lastPointerY;
    this.lastPointerX = event.clientX;
    this.lastPointerY = event.clientY;
    this.mouseDX += dx;
    this.mouseDY += dy;
  };

  private onPointerUp = (): void => {
    this.pointerHandler?.up();
    this.dragging = false;
    this.canvas.style.cursor = 'grab';
  };

  private onWheel = (event: WheelEvent): void => {
    event.preventDefault();
    if (this.pointerHandler?.wheel(event.deltaY)) return;
    this.wheelDelta += event.deltaY;
  };

  /** Poll the gamepad; call once per rendered frame. */
  pollGamepad(): void {
    if (this.gamepadIndex === null) {
      const pads = navigator.getGamepads?.();
      if (!pads) return;
      const found = Array.from(pads).find((pad) => pad && pad.connected);
      if (found) {
        this.gamepadIndex = found.index;
        this.gamepadConnected = true;
      }
    }
    if (this.gamepadIndex === null) return;

    const pad = navigator.getGamepads?.()[this.gamepadIndex];
    if (!pad) return;

    const buttons = pad.buttons.map((button) => button.pressed || button.value > 0.5);
    if (this.padButtons.length > 0) {
      if (buttons[9] && !this.padButtons[9]) this.startPressed = true;
      if (buttons[1] && !this.padButtons[1]) this.pendingB = true;
      if ((buttons[2] || buttons[3] || buttons[6] || buttons[7]) && !(this.padButtons[2] || this.padButtons[3] || this.padButtons[6] || this.padButtons[7])) {
        this.pendingZ = true;
      }
      if (buttons[0] && !this.padButtons[0]) this.pendingA = true;
      if (buttons[1] && !this.padButtons[1]) this.anyInput = true;
    }
    this.padButtons = buttons;
    this.padAxes = [
      pad.axes[0] ?? 0,
      pad.axes[1] ?? 0,
      pad.axes[2] ?? 0,
      pad.axes[3] ?? 0,
    ];
    if (Math.hypot(this.padAxes[0], this.padAxes[1]) > 0.2) this.anyInput = true;
  }

  consumeStart(): boolean {
    const pressed = this.startPressed;
    this.startPressed = false;
    return pressed;
  }

  private keyboardStick(): { x: number; y: number; mag: number } {
    let x = 0;
    let y = 0;
    if (this.keys.has('KeyD')) x += 1;
    if (this.keys.has('KeyA')) x -= 1;
    if (this.keys.has('KeyW')) y += 1;
    if (this.keys.has('KeyS')) y -= 1;

    if (x === 0 && y === 0) return { x: 0, y: 0, mag: 0 };

    const length = Math.hypot(x, y);
    const walk = this.keys.has('AltLeft') || this.keys.has('AltRight');
    const mag = walk ? 32 : 64;
    return { x: (x / length) * mag, y: (y / length) * mag, mag };
  }

  private gamepadStick(): { x: number; y: number; mag: number } | null {
    if (this.gamepadIndex === null) return null;
    let x = this.padAxes[0];
    let y = -this.padAxes[1];
    const magnitude = Math.hypot(x, y);
    if (magnitude < 0.16) return { x: 0, y: 0, mag: 0 };

    // Radial deadzone with rescaling.
    const scaled = Math.min(1, (magnitude - 0.16) / 0.84);
    x = (x / magnitude) * scaled * 64;
    y = (y / magnitude) * scaled * 64;
    return { x, y, mag: scaled * 64 };
  }

  private gamepadDown(): number {
    if (this.gamepadIndex === null) return 0;
    let buttons = 0;
    if (this.padButtons[0]) buttons |= A_BUTTON;
    if (this.padButtons[1]) buttons |= B_BUTTON;
    if (this.padButtons[2] || this.padButtons[3] || this.padButtons[6] || this.padButtons[7]) {
      buttons |= Z_TRIG;
    }
    return buttons;
  }

  private keyboardDown(): number {
    let buttons = 0;
    if (this.keys.has('Space')) buttons |= A_BUTTON;
    if (this.keys.has('ShiftLeft') || this.keys.has('ShiftRight') || this.keys.has('KeyJ')) {
      buttons |= B_BUTTON;
    }
    if (
      this.keys.has('ControlLeft') ||
      this.keys.has('ControlRight') ||
      this.keys.has('KeyK')
    ) {
      buttons |= Z_TRIG;
    }
    return buttons;
  }

  /** Build the controller snapshot for one 30 Hz physics step. */
  consumeStep(): ControllerSnapshot {
    const pad = this.gamepadStick();
    const keyboard = this.keyboardStick();
    const stick = pad && pad.mag > 0 ? pad : keyboard;

    let buttonDown = this.keyboardDown() | this.gamepadDown();
    let buttonPressed = 0;
    if (this.pendingA) buttonPressed |= A_BUTTON;
    if (this.pendingB) buttonPressed |= B_BUTTON;
    if (this.pendingZ) buttonPressed |= Z_TRIG;

    // Keep held state coherent with the latched edge.
    if (buttonPressed & A_BUTTON) buttonDown |= A_BUTTON;
    if (buttonPressed & B_BUTTON) buttonDown |= B_BUTTON;
    if (buttonPressed & Z_TRIG) buttonDown |= Z_TRIG;

    this.pendingA = false;
    this.pendingB = false;
    this.pendingZ = false;

    return {
      stickX: stick.x,
      stickY: stick.y,
      stickMag: stick.mag,
      buttonDown,
      buttonPressed,
    };
  }

  /** Camera deltas for the current render frame. */
  consumeCameraInput(): CameraInput {
    const keyboardOrbitX =
      (this.keys.has('KeyE') || this.keys.has('ArrowRight') ? 1 : 0) -
      (this.keys.has('KeyQ') || this.keys.has('ArrowLeft') ? 1 : 0);
    const keyboardOrbitY =
      (this.keys.has('ArrowDown') ? 1 : 0) - (this.keys.has('ArrowUp') ? 1 : 0);

    let orbitX = keyboardOrbitX;
    let orbitY = keyboardOrbitY;
    if (this.gamepadIndex !== null) {
      const rx = this.padAxes[2] ?? 0;
      const ry = this.padAxes[3] ?? 0;
      if (Math.abs(rx) > 0.16) orbitX += rx;
      if (Math.abs(ry) > 0.16) orbitY += ry;
    }

    const result: CameraInput = {
      dx: this.mouseDX,
      dy: this.mouseDY,
      wheel: this.wheelDelta,
      orbitX,
      orbitY,
    };
    this.mouseDX = 0;
    this.mouseDY = 0;
    this.wheelDelta = 0;
    return result;
  }

  isPressed(code: string): boolean {
    return this.justPressed.has(code);
  }

  consume(code: string): boolean {
    if (!this.justPressed.has(code)) return false;
    this.justPressed.delete(code);
    return true;
  }

  get moveHeld(): boolean {
    for (const code of MOVE_KEYS) if (this.keys.has(code)) return true;
    return false;
  }
}
