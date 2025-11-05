# Blaulicht

A real-time DMX lighting control system with plugin support, built in Rust.

## Features

### Core Functionality
- **DMX512 control** via Art-Net or USB interfaces
- **Real-time audio analysis** with FFT for reactive lighting
- **Scene management** with smooth transitions
- **WebSocket API** for external control
- **Plugin system** with WASM-based plugins
- **Web-based UI** for remote control

### Built-in Plugins

#### 3D Visualizer
Real-time 3D visualization of your lighting rig:
- Interactive 3D scene with perspective camera
- Solid fixture rendering with volumetric beams
- 6-axis control: translate (X/Y/Z) and rotate (X/Y/Z)
- Real-time pan/tilt and color visualization
- Dynamic floor lighting from fixture beams
- See [Visualizer README](crates/plugins/visualizer/README.md)

#### Drums Sequencer
MIDI-triggered scene sequencer:
- Connect to any MIDI device
- Multiple sequencers with custom names
- Scene-based step sequencing
- Multi-note triggers per sequencer
- See [Drums Plugin README](crates/plugins/drums/README.md)

#### MIDI Controllers
- **Korg nanoKONTROL** mapping plugin for fader/knob control

### Fixture Support
- Moving heads with pan/tilt
- RGB/RGBW lights
- Dimmers
- Extensible fixture definitions

## Architecture

```
blaulicht/
├── crates/
│   ├── core/          # Main engine and UI
│   ├── shared/        # Common types and fixtures
│   ├── plugin_framework/  # WASM plugin API
│   └── plugins/       # Plugin implementations
│       ├── visualizer/       # 3D visualization
│       ├── drums/            # MIDI sequencer
│       └── midi_korg_nano_kontrol/  # Controller mapping
```

## Building

### Prerequisites
- Rust nightly toolchain
- For WASM plugins: `wasm32-unknown-unknown` target

### Build Steps

```bash
# Install WASM target
rustup target add wasm32-unknown-unknown

# Build main application
cargo build --release

# Build all plugins
cd crates/plugins
make
```

## Running

```bash
# Start the application
cargo run --release

# Or run the built binary
./target/release/blaulicht
```

The web UI will be available at the configured port (default: http://localhost:8080).

## Configuration

Edit `config.toml` to configure:
- DMX output (Art-Net address, universe)
- Audio input device
- Network settings
- Default scenes
- Plugin paths

## Plugin Development

Plugins are built as WASM modules using the Blaulicht plugin framework.

See the [plugin framework documentation](crates/plugin_framework/) and example plugins for details.

### Basic Plugin Structure

```rust
use blaulicht_plugin_framework::prelude::*;

pub struct MyPlugin {
    // plugin state
}

impl Plugin for MyPlugin {
    fn tick(&mut self, input: &TickInput) {
        // Called every frame
    }
    
    fn ui(&mut self) {
        // Render UI using egui-style API
    }
    
    fn control_event(&mut self, event: &ControlEvent) {
        // Handle UI and MIDI events
    }
}
```

## API

### WebSocket API
Connect to `ws://localhost:8080/ws` for real-time control:
- Scene activation
- Fixture control
- State queries
- Event streaming

### REST API
- `GET /api/state` - Current system state
- `POST /api/scene/{id}` - Activate scene
- `GET /api/fixtures` - List fixtures

## License

Dual licensed under MIT or Apache-2.0.

## Contributing

Contributions welcome! Please open an issue or PR.

Areas for contribution:
- New fixture definitions
- Plugin development
- UI improvements
- Documentation
- Bug fixes
