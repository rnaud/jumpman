# Project requirements

- Focus on the Rust/Bevy native version (`native/`). The Three.js browser version (`src/`) is frozen: do not develop, fix or test new features there. Do not delete it unless the user asks.
- Reuse shared asset exports where possible and verify the native build with appropriate checks (`cargo test`, desktop run, iOS simulator).
- If something cannot be completed or tested, explicitly report that limitation.
- API keys for image generation and similar tasks live in `~/projects/.env` (variables `OPENAI_API_KEY`, `GEMINI_API_KEY`). Load them from that file at run time; never print, log, commit or paste the values. OpenAI image generation: use `gpt-image-2` (`gpt-image-1` is not available on this account); list what the key can use with `GET /v1/models`.
