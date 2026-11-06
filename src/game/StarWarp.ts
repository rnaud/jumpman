import marioMask from '../../assets/extracted/textures/segment2/segment2.10458.ia8.png';
import halfBowserMask from '../../assets/extracted/textures/segment2/segment2.11458.ia8.png';
import halfCircleMask from '../../assets/extracted/textures/segment2/segment2.0FC58.ia8.png';
import halfStarMask from '../../assets/extracted/textures/segment2/segment2.0F458.ia8.png';

function texture(url: string): HTMLImageElement {
  const image = new Image();
  image.src = url;
  return image;
}

/** screen_transition.c uses a clamped Mario texture and a mirrored half-star. */
export class StarWarp {
  private readonly bowser = texture(halfBowserMask);
  private readonly circle = texture(halfCircleMask);
  private readonly mario = texture(marioMask);
  private readonly star = texture(halfStarMask);
  private readonly canvas = document.createElement('canvas');
  private readonly ctx = this.canvas.getContext('2d')!;

  constructor(gameCanvas: HTMLCanvasElement) {
    this.canvas.id = 'star-warp';
    this.canvas.hidden = true;
    this.canvas.setAttribute('aria-hidden', 'true');
    gameCanvas.after(this.canvas);
  }

  clear(): void { this.canvas.hidden = true; }

  draw(opening: boolean, frame: number, width: number, height: number, closing: 'mario' | 'bowser' | 'circle' = 'mario'): void {
    const canvas = this.canvas;
    const ctx = this.ctx;
    canvas.hidden = false;
    // The original projection is 240 units high and centers the wipe on screen.
    const w = Math.max(1, Math.round(240 * width / Math.max(1,height)));
    if (canvas.width !== w || canvas.height !== 240) {
      canvas.width = w;
      canvas.height = 240;
    }
    canvas.style.width = width + 'px';
    canvas.style.height = height + 'px';
    const full = Math.max(w,240);
    const duration = closing === 'bowser' ? 48 : closing === 'circle' ? 20 : 32;
    const end = closing === 'circle' ? 0 : 16;
    const radius = Math.round(opening ? full * frame / 15 : full + (end-full) * frame / (duration-1));
    const x = Math.round(w / 2) - radius;
    const y = 120 - radius;
    const size = radius * 2;
    ctx.fillStyle = 'black';
    ctx.fillRect(0,0,w,240);
    const image = opening ? this.star : this[closing];
    if (!radius || !image.naturalWidth) return;
    ctx.clearRect(x,y,size,size);
    if (!opening && closing === 'mario') {
      ctx.drawImage(image,x,y,size,size);
      return;
    }
    ctx.drawImage(image,x+radius,y,radius,size);
    ctx.save();
    ctx.translate(x+radius,y);
    ctx.scale(-1,1);
    ctx.drawImage(image,0,0,radius,size);
    ctx.restore();
  }
}
