# Midilight

A 16-sixteenth-note brightness roll for Blaulicht. Select fixtures/groups, create
an animation of type WASM, and choose **Midilight**. In the template editor,
fixture selection supplies the available targets. Apply the edited template
before adding it to a scene.

- Click or drag empty grid space to create notes. Drag note centers to move them,
  or their edges to resize. Notes snap to sixteenths and cannot overlap in a row.
- Gate holds the global maximum brightness; Sine makes a half-sine pulse across
  each note. The output is Alpha; colors are unchanged. Gaps output zero.
- Toggle Groups to drive each mapped group together. Select a row by clicking its
  label; the target arrows cycle that row through Unassigned and every
  fixture/group no other row holds, so a mapping can always be cleared with the
  same arrows that set it. Unassign clears it outright. Add/delete rows and use pagination for larger selections.
- Changing the selection or fixture/group mode preserves notes but clears all
  mappings. Playback stays off until every remaining row has a unique target.
- Reverse rows alternates the row-to-target mapping after N complete bars.
- Pro DJ Link supplies bar alignment when audio sync is enabled. Sources without
  bar position anchor the roll to the first beat and log a warning once on entry
  into estimated alignment. Missing tempo stops output until a fresh beat arrives.
- The roll stays locked to four musical beats, independent of scene/animation
  speed multipliers. Pausing suppresses output while the clock continues to sync.

Pattern and controls use per-instance showfile state. Save the showfile to retain
changes across sessions. No MIDI device or MIDI file is required.

Build from the repository flake environment:

```sh
direnv exec . cargo test --manifest-path crates/plugins/midilight/Cargo.toml
direnv exec . make -C crates/plugins build_noopt
```
