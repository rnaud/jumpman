//! The SM64 sequence player + RSP mixer (`assets/audio/music/sm64-us.wasm`), run inside the
//! `wasmi` interpreter exactly as `src/game/music-worklet.js` runs it inside an AudioWorklet.
//! Engine calls take loosely typed numbers (the JS bridge passes doubles); each argument is coerced
//! to the export's declared wasm type.
use wasmi::{Engine, Func, Instance, Linker, Memory, Module, Store, Val, ValType};

/// Native mixer rate of the decomp audio engine.
pub const ENGINE_RATE: u32 = 32000;
/// Frames per `music_render` call are at most this many (stereo i16 each).
pub const PCM_FRAMES: usize = 544;

pub struct Sm64Engine {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    pcm_ptr: usize,
}

impl Sm64Engine {
    pub fn new(wasm: &[u8]) -> Result<Self, String> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).map_err(|e| e.to_string())?;
        let mut store = Store::new(&engine, ());
        let linker = <Linker<()>>::new(&engine);
        let instance = linker.instantiate_and_start(&mut store, &module).map_err(|e| e.to_string())?;
        let memory = instance.get_memory(&store, "memory").ok_or("wasm has no memory export")?;
        let mut me = Self { store, instance, memory, pcm_ptr: 0 };
        if me.func("_initialize").is_some() {
            me.call("_initialize", &[])?;
        }
        me.call("music_init", &[])?;
        me.pcm_ptr = me.call("music_samples", &[])?.first().copied().unwrap_or(0.0) as u32 as usize;
        Ok(me)
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
                    ValType::I32 => Val::I32(js_to_int32(v)),
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

    /// Render the next block; returns the number of stereo frames now readable via [`Self::pcm`].
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

/// JS `ToInt32` for the loose numeric bridge (wasm i32 params receive wrapped values).
fn js_to_int32(v: f64) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    (v.trunc().rem_euclid(4294967296.0) as u32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int32_wraps_like_js() {
        assert_eq!(js_to_int32(2153250705.0), 2153250705u32 as i32);
        assert_eq!(js_to_int32(-3.7), -3);
    }
}
