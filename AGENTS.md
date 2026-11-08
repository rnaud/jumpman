# Project requirements

- Focus on the Rust/Bevy native version (`native/`). The Three.js browser version (`src/`) is frozen: do not develop, fix or test new features there. Do not delete it unless the user asks.
- Reuse shared asset exports where possible and verify the native build with appropriate checks (`cargo test`, desktop run, iOS simulator).
- If something cannot be completed or tested, explicitly report that limitation.
