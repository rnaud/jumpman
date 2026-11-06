import { A_BUTTON, type ControllerSnapshot } from '../sim/state';

export interface CompletedCourse { title: string; heading: string; coins: number; highScore: boolean; key: boolean }
export type SaveChoice = 'save' | 'quit' | 'continue';
const choices: SaveChoice[] = ['save', 'quit', 'continue'];

export class CourseResult {
  private readonly root = document.querySelector<HTMLElement>('#course-result')!;
  private readonly count = this.root.querySelector<HTMLElement>('.result-coins')!;
  private readonly record = this.root.querySelector<HTMLElement>('.result-record')!;
  private readonly options = this.root.querySelector<HTMLElement>('.result-options')!;
  private readonly buttons = [...this.options.querySelectorAll('button')];
  private total = 0;
  private shown = 0;
  private timer = 0;
  private highScore = false;
  private selection = 0;
  private lastDirection = 0;
  private repeat = 0;

  constructor(private readonly finish: (choice: SaveChoice) => void) {
    this.buttons.forEach((button,i) => button.addEventListener('click', () => this.confirm(i)));
  }

  get visible(): boolean { return !this.root.hidden; }

  show(course: CompletedCourse): void {
    this.total = course.coins;
    this.highScore = course.highScore;
    this.shown = this.timer = this.selection = this.lastDirection = this.repeat = 0;
    this.root.querySelector('strong')!.textContent = course.heading;
    this.root.querySelector('h2')!.textContent = course.title;
    this.count.textContent = '× 0';
    this.record.hidden = this.options.hidden = true;
    this.buttons.forEach((button,i) => {
      button.disabled = true;
      button.setAttribute('aria-current', String(i === 0));
    });
    this.root.hidden = false;
  }

  hide(): void { this.root.hidden = true; }

  confirm(index = this.selection): void {
    if (this.visible && !this.options.hidden && this.timer > 110) this.finish(choices[index]);
  }

  step(input: ControllerSnapshot, coinSound: (count: number, record: boolean) => void): void {
    const complete = this.shown >= this.total;
    if (!complete && ((this.timer & 1) || this.total > 70)) {
      this.count.textContent = '× ' + ++this.shown;
      coinSound(this.shown, this.highScore && this.shown === this.total);
    }
    this.record.hidden = !complete || !this.highScore;
    if (complete && this.timer > 100) this.options.hidden = false;
    if (!this.options.hidden) {
      if (this.timer > 110 && this.buttons[0].disabled) this.buttons.forEach(button => { button.disabled = false; });
      const direction = Math.abs(input.stickY) > 30 ? -Math.sign(input.stickY) : 0;
      if (direction && (direction !== this.lastDirection || ++this.repeat >= 10)) {
        this.selection = Math.max(0, Math.min(2, this.selection + direction));
        this.buttons.forEach((button,i) => button.setAttribute('aria-current', String(i === this.selection)));
        this.repeat = 0;
      }
      this.lastDirection = direction;
      if (input.buttonPressed & A_BUTTON) this.confirm();
    }
    this.timer++;
  }
}
