//! The Mario Kart 64 sound library (`mk64-audio.wasm`: the decomp's `src/audio/*.c` plus a portable RSP
//! mixer, ROM data patched in at import time) run inside `wasmi`. One [`Mk64Module`] is compiled once; every
//! [`Mk64Engine`] is a fresh instance of it (a hard reset of the whole library takes a few milliseconds).
use wasmi::{Engine, Func, Instance, Linker, Memory, Module, Store, Val, ValType};

/// Output rate of the sound library's first audio session preset (`gAudioSessionPresets[0]`).
pub const ENGINE_RATE: u32 = 26800;
/// Upper bound of the stereo frames one `music_render` call returns (the AI buffer of one video frame).
pub const PCM_FRAMES: usize = 1024;

#[derive(Clone)]
pub struct Mk64Module {
    engine: Engine,
    module: Module,
}

impl Mk64Module {
    pub fn load(wasm: &[u8]) -> Result<Self, String> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).map_err(|e| e.to_string())?;
        Ok(Self { engine, module })
    }

    pub fn instantiate(&self) -> Result<Mk64Engine, String> {
        let mut store = Store::new(&self.engine, ());
        let linker = <Linker<()>>::new(&self.engine);
        let instance = linker.instantiate_and_start(&mut store, &self.module).map_err(|e| e.to_string())?;
        let memory = instance.get_memory(&store, "memory").ok_or("wasm has no memory export")?;
        let mut me = Mk64Engine { store, instance, memory, pcm_ptr: 0 };
        if me.func("_initialize").is_some() {
            me.call("_initialize", &[])?;
        }
        let rate = me.call("music_init", &[0.0])?.first().copied().unwrap_or(0.0) as u32;
        if rate != ENGINE_RATE {
            return Err(format!("unexpected mixer rate {rate} (expected {ENGINE_RATE})"));
        }
        me.pcm_ptr = me.call("music_samples", &[])?.first().copied().unwrap_or(0.0) as u32 as usize;
        Ok(me)
    }
}

pub struct Mk64Engine {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    pcm_ptr: usize,
}

/// State of one sequence player (0 = course music, 1 = events / jingles, 2 = sound effects).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub enabled: bool,
    pub sequence: u32,
    /// Beats per minute as the sequence set it (before the game's tempo ramps).
    pub tempo: u32,
    /// Fade volume 0..1.
    pub volume: f32,
}

impl Mk64Engine {
    /// Does the library export `name`?
    pub fn has_export(&self, name: &str) -> bool {
        self.func(name).is_some()
    }

    fn func(&self, name: &str) -> Option<Func> {
        self.instance.get_func(&self.store, name)
    }

    /// Call an export with numeric arguments; missing trailing arguments are zero.
    pub fn call(&mut self, name: &str, args: &[f64]) -> Result<Vec<f64>, String> {
        let f = self.func(name).ok_or_else(|| format!("missing export {name}"))?;
        let ty = f.ty(&self.store);
        let params: Vec<Val> = ty
            .params()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let v = args.get(i).copied().unwrap_or(0.0);
                match t {
                    ValType::I32 => Val::I32(v.rem_euclid(4294967296.0) as u32 as i32),
                    ValType::I64 => Val::I64(v as i64),
                    ValType::F32 => Val::F32((v as f32).into()),
                    _ => Val::F64(v.into()),
                }
            })
            .collect();
        let mut results: Vec<Val> = ty.results().iter().map(|t| Val::default_for_ty(*t)).collect();
        f.call(&mut self.store, &params, &mut results).map_err(|e| format!("{name}: {e}"))?;
        Ok(results
            .iter()
            .map(|v| match v {
                Val::I32(i) => *i as f64,
                Val::I64(i) => *i as f64,
                Val::F32(f) => f32::from(*f) as f64,
                Val::F64(f) => f64::from(*f),
                _ => 0.0,
            })
            .collect())
    }

    fn run(&mut self, name: &str, args: &[f64]) {
        let _ = self.call(name, args);
    }

    /// `play_sequence`: the course theme on player 0 (the decomp's `SEQ_*` id).
    pub fn track(&mut self, sequence: u32) {
        self.run("music_track", &[sequence as f64]);
    }
    /// `play_sequence2`: an event / jingle on player 1.
    pub fn event(&mut self, sequence: u32) {
        self.run("music_event", &[sequence as f64]);
    }
    /// A raw word for the game-side command queue (`func_800C3448`).
    pub fn command(&mut self, word: u32) {
        self.run("music_command", &[word as f64]);
    }
    /// Race mode (`gModeSelection`: 0 Grand Prix), the player's rank (0 = first) and the course id.
    pub fn mode(&mut self, mode: u32, rank: u32, course: u32) {
        self.run("music_mode", &[mode as f64, rank as f64, course as f64]);
    }
    pub fn final_lap(&mut self) {
        self.run("music_final_lap", &[]);
    }
    pub fn star(&mut self, on: bool) {
        self.run(if on { "music_star_start" } else { "music_star_end" }, &[]);
    }
    pub fn lightning(&mut self, on: bool) {
        self.run(if on { "music_lightning_start" } else { "music_lightning_end" }, &[]);
    }
    pub fn finish(&mut self) {
        self.run("music_finish", &[]);
    }

    pub fn player(&mut self, p: u32) -> PlayerState {
        let get = |me: &mut Self, name: &str| me.call(name, &[p as f64]).ok().and_then(|v| v.first().copied()).unwrap_or(0.0);
        PlayerState {
            enabled: get(self, "music_player_enabled") != 0.0,
            sequence: get(self, "music_player_seq") as u32,
            tempo: get(self, "music_player_tempo") as u32,
            volume: get(self, "music_player_volume") as f32,
        }
    }

    /// Unknown microcode commands seen so far (0 for a healthy build).
    pub fn unknown_ops(&mut self) -> u32 {
        self.call("music_unknown_ops", &[]).ok().and_then(|v| v.first().copied()).unwrap_or(0.0) as u32
    }

    /// One video frame of audio: the game-side update, then the audio thread's task. Returns the number of
    /// stereo frames now readable through [`Self::pcm`] (about 446 at 26.8 kHz).
    pub fn render(&mut self) -> usize {
        let n = self.call("music_render", &[]).ok().and_then(|r| r.first().copied()).unwrap_or(0.0) as usize;
        n.min(PCM_FRAMES)
    }

    /// Interleaved stereo i16 produced by the last [`Self::render`].
    pub fn pcm(&self, frames: usize, out: &mut Vec<i16>) {
        out.clear();
        out.resize(frames * 2, 0);
        let data = self.memory.data(&self.store);
        for (i, o) in out.iter_mut().enumerate() {
            let at = self.pcm_ptr + i * 2;
            *o = i16::from_le_bytes([data[at], data[at + 1]]);
        }
    }
}
