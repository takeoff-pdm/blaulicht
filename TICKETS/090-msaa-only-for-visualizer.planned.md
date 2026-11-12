# 090 — Multisampling and depth buffer only for the visualizer, not the whole window

| | |
|---|---|
| Epic | Ops / show host (performance) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/desk.rs`, `crates/ui_egui/src/app/pages/visualizer/{gl,renderer,state,ui}.rs`

## Problem
The main window is created with 4× MSAA and a 24-bit depth buffer (`crates/ui_egui/src/desk.rs:31-32`):

```rust
// The visualizer is rendered in an egui GL callback and requires a
// real depth attachment for opaque fixture geometry and outlines.
depth_buffer: 24,
multisampling: 4,
```

Only the visualizer's 3D view needs this, but it applies to the default framebuffer, so **every frame on every page**
is rendered and resolved with 4× MSAA. The UI also redraws at a fixed ~60 fps
(`crates/ui_egui/src/app/ui.rs:103`, `request_repaint_after(16 ms)`), so this cost is paid constantly.

Measured on the show host `bl` (2026-11-12, i5-4310M with Intel HD 4600, Blaulicht running): the UI thread uses
10–22 % of a CPU and graphics interrupts stay at 80–95/s whether the UI is idle or used heavily, i.e. the cost is the
fixed frame rate times the per-frame cost. `bl` has only 2 real cores, and the DMX/main loop share them with the UI.
4× MSAA on the full window resolution (plus the external screen) costs fill rate and memory bandwidth on this old iGPU.

The GL context's sample count is fixed when eframe creates the window, so "turn MSAA on when the visualizer opens"
doesn't work at runtime. The visualizer needs its own multisampled target instead.

## Scope
- Set `multisampling: 0` and `depth_buffer: 0` in `NativeOptions`.
- Render the visualizer into its own offscreen framebuffer: multisampled color renderbuffer + 24-bit depth renderbuffer
  (`glRenderbufferStorageMultisample`, 4 samples), sized to the visualizer rect in physical pixels and reallocated on
  resize. Resolve with `glBlitFramebuffer` into a texture or straight into the egui default framebuffer region, then
  restore egui's GL state (bound framebuffer, viewport, scissor, blend, depth test) as the callback does today.
- Keep a fallback: if creating the multisampled FBO fails (driver limits), render without MSAA instead of failing.
- Optional: a `config.toml` key `visualizer_msaa_samples` (0/2/4, default 4) for weak machines.

## Out of scope
- The fixed 60 fps redraw (`ui.rs:103`). Separate ticket: lower it or repaint on demand.
- Visualizer rendering changes beyond the render target.

## Key code
- Window options — `crates/ui_egui/src/desk.rs:27-33`
- Visualizer GL callback — `crates/ui_egui/src/app/pages/visualizer/gl.rs`, `renderer.rs`
- The comment in `desk.rs` explains why depth is needed (opaque fixture geometry and outlines); the offscreen depth
  buffer must cover the same need.

## Acceptance criteria
- [ ] The visualizer looks the same as before (anti-aliased edges, correct depth ordering, outlines), including after
      resizing the window and on the external screen.
- [ ] Pages without the visualizer render without MSAA (verify with `glGetIntegerv(GL_SAMPLES)` on the default
      framebuffer = 0, or a debug log at startup).
- [ ] No GL errors or state leaks into egui (text and widgets render correctly after visiting the visualizer).

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- Screenshots of the visualizer before and after (`scripts/kt` can't render GL; use blenv or the real app).
- On `bl`: compare the UI thread's CPU use and the GPU load before and after, with the same page open, e.g. with the
  sampler used on 2026-11-12 (per-thread `/proc/<pid>/task/*/schedstat` deltas) or `intel_gpu_top`.

## Notes
