# Drums Plugin

A MIDI-triggered scene sequencer plugin for Blaulicht that allows triggering scene sequences using MIDI note messages.

## Features

### MIDI Integration
- **Connect to any MIDI device** from a dropdown list
- **Real-time MIDI monitoring** shows recent messages
- **Multiple note triggers** - each sequencer can respond to multiple MIDI notes

### Sequencer Management
- **Create multiple sequencers** with custom names
- **Rename sequencers** at any time
- **Delete sequencers** when no longer needed
- **Demo sequencers** provided on first launch (Kick Drum, Snare, Hi-Hat)

### Step Sequencing
- **Add/remove steps** for each sequencer
- **Scene-based steps** - each step triggers a Blaulicht scene
- **Current step indicator** shows active position in sequence
- **Automatic advancement** when triggered by MIDI

### User Interface
- **Overview mode** - Lists all sequencers
- **Edit mode** - Manage steps for a specific sequencer
- **Add/Edit step dialogs** for scene selection
- **MIDI note configuration** with comma-separated input (e.g., "36,38,40")

## Usage

### Initial Setup
1. Load the plugin in Blaulicht
2. Select your MIDI device from the dropdown
3. Three demo sequencers are created automatically:
   - Kick Drum (notes 38, 40)
   - Snare (notes 36, 39)
   - Hi-Hat (notes 42, 44)

### Creating a Sequencer
1. Click **"New Sequencer"** button
2. Enter a name for your sequencer
3. Add MIDI notes (comma-separated, 0-127)
4. Click **"Create"**

### Adding Steps
1. Select a sequencer from the overview
2. Click **"Add Step"**
3. Enter the scene index to trigger
4. Click **"Add"**

### Editing Steps
1. In sequencer edit mode, click **"Edit"** next to a step
2. Modify the scene index
3. Click **"Save"**

### Triggering Sequences
- Send a MIDI note message that matches a sequencer's configured notes
- The sequencer advances to the next step
- The corresponding scene is activated in Blaulicht
- Sequence wraps around after the last step

## Technical Details

- Built using the Blaulicht plugin framework
- MIDI connection handling via plugin framework MIDI API
- State persistence (sequencers and steps saved/restored)
- Scene lookup and name display from Blaulicht DMX state

## Default Demo Sequencers

**Kick Drum** (MIDI notes: 38, 40)
- 8-step sequence cycling through scenes 0-7

**Snare** (MIDI notes: 36, 39)
- 4-step sequence: scenes 0, 2, 3, 1

**Hi-Hat** (MIDI notes: 42, 44)
- 4-step sequence: scenes 1, 0, 1, 0

These can be deleted or modified as needed.

## MIDI Note Format

Enter MIDI notes as comma-separated values:
- Valid range: 0-127
- Example: `36,38,40` for kick drum variants
- Example: `60` for middle C
- Invalid values are ignored
