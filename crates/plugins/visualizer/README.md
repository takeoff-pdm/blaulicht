# Bevy Visualizer Plugin

A 3D visualization plugin for Blaulicht that provides real-time rendering of DMX fixtures in a 3D environment.

## Features

### 3D Scene Rendering
- **Perspective projection** with configurable camera controls
- **Orbit camera** - Click and drag to rotate view
- **Zoom** - Pinch or scroll to adjust camera distance
- **Grid floor** with dynamic lighting from fixtures
- **Volumetric beams** with realistic cone projection

### Fixture Visualization
- **Solid 3D cuboid representation** of each fixture
- **Back-face culling** for proper depth rendering
- **Ambient occlusion** for realistic shading
- **Real-time pan/tilt visualization** with beam direction indicators
- **Color-accurate rendering** based on DMX fixture state

### Interactive Controls
- **Click to select** fixtures
- **3-axis translation handles**:
  - Red (X-axis)
  - Green (Y-axis)
  - Blue (Z-axis)
- **3-axis rotation handles**:
  - Red circle (X-axis rotation)
  - Green circle (Y-axis rotation)
  - Blue circle (Z-axis rotation)
- **Drag handles** to adjust fixture position and orientation in 3D space
- **Yellow outline** shows selected fixture (rotates with fixture)

### Display Options
- Toggle fixture labels (group:fixture ID)
- Toggle grid display
- Autoframe button to reset view

## Usage

1. Load the plugin in Blaulicht
2. Fixtures will automatically appear in the 3D view based on their DMX state
3. Click a fixture to select it
4. Drag the colored handles to position or rotate fixtures
5. Click and drag background to orbit camera
6. Pinch/scroll to zoom

## Technical Details

- Built using the Blaulicht plugin framework
- Renders to a scalable canvas that fills the plugin container
- Custom 3D projection and rotation matrices
- Real-time lighting calculations for floor illumination
- Depth-sorted rendering for correct visibility

## Configuration

The plugin provides default settings for:
- Canvas size: 800x600
- Camera rotation: 30° horizontal, 45° vertical
- Camera distance: 500 units
- Default fixture pan/tilt: 0°

All settings can be adjusted in real-time through the UI.
