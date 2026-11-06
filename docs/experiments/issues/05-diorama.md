# 05 — Diorama

**Concept:** a poly-pocket diorama viewed from far away. Real Marios and Goombas move on their own; the player flies the camera and pokes the cast.

## Acceptance

- [x] Miniature board built from real SM64 assets (Bob-omb Battlefield geometry, trees, Mario and Goomba models).
- [x] Free-fly FPS camera: drag look, WASD movement, Q/E vertical, Shift boost.
- [x] Autonomous Marios and Goombas wander on the board.
- [x] Click a Mario to jump; click a Goomba to make it chase the nearest Mario.
- [x] Selection ring and HUD toast feedback.
- [x] Enter with **V** or the title-screen **DIORAMA** button; leave with **V**.
- [x] Browser smoke validates loading, actor count, selection and mode exit.

## Known limits

- The board is an arrangement of the Bob-omb Battlefield visual mesh rather than a hand-authored miniature set.
- Actors use stylized merged Mario/Goomba meshes; they do not yet run the full animation state machine.
