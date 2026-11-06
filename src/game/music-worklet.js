// Run the decomp sequence interpreter and RSP mixer on the audio clock, independently of rendering FPS.
class Sm64MusicProcessor extends AudioWorkletProcessor {
  constructor({processorOptions}) {
    super();
    this.engine = new WebAssembly.Instance(processorOptions.module, {}).exports;
    this.engine._initialize();
    this.engine.music_init();
    this.pcm = new Int16Array(this.engine.memory.buffer, this.engine.music_samples(), 544 * 2);
    this.step = 32000 / sampleRate;
    this.frames = this.index = this.phase = 0;
    this.left = this.right = this.nextLeft = this.nextRight = 0;
    this.playing = false;
    this.port.onmessage = ({data}) => {
      if (data.location) this.engine.music_location(...data.location);
      if (data.sound) this.engine.sound_effect(...data.sound);
      if (data.birds) data.birds.forEach((pos,i) => this.engine.music_bird(i,...pos));
      if (data.track) {
        this.engine.music_start(data.track.sequence, data.track.preset);
        this.frames = this.index = this.phase = 0;
        this.left = this.right = this.nextLeft = this.nextRight = 0;
        this.playing = true;
      }
      if (data.power) this.engine.music_power(...data.power);
      switch (data.event) {
        case 'clear': this.engine.play_course_clear(); break;
        case 'spawn': this.engine.play_power_star_jingle(1); break;
        case 'puzzle': this.engine.play_puzzle_jingle(); break;
        case 'raceStart': this.engine.music_race(1); break;
        case 'raceEnd': this.engine.music_race(0); break;
      }
      if (data.stop) this.engine.seq_player_fade_out(0,126);
    };
  }

  advance() {
    if (this.index === this.frames) {
      this.frames = this.engine.music_render();
      this.index = 0;
    }
    this.left = this.nextLeft;
    this.right = this.nextRight;
    this.nextLeft = this.pcm[this.index * 2] / 32768;
    this.nextRight = this.pcm[this.index * 2 + 1] / 32768;
    this.index++;
  }

  process(inputs, outputs) {
    if (!this.playing) return true;
    const [left, right] = outputs[0];
    for (let i = 0; i < left.length; i++) {
      left[i] = this.left + (this.nextLeft - this.left) * this.phase;
      right[i] = this.right + (this.nextRight - this.right) * this.phase;
      this.phase += this.step;
      while (this.phase >= 1) { this.phase--; this.advance(); }
    }
    return true;
  }
}
registerProcessor('sm64-music', Sm64MusicProcessor);
