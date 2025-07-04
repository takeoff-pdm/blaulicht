export interface DMXData {
  // Length = 255.
  channels: Uint8Array;
}

export interface Data {
  logs: string[];
  plugins: { number: PluginState };
  dmx_engine: EngineState;
}

export interface PluginState {
  path: string;
  flags: PluginFlags;
  logs: string[];
}

export interface PluginFlags {
  enabled: boolean;
  has_error: boolean;
}

export interface EngineState {
  groups: { number: FixtureGroup };
  selection: EngineSelection;
}

export interface FixtureGroup {
  fixtures: { number: Fixture };
}

export interface Fixture {
  name: string;
  type_: FixtureType;
  state: FixtureState;
}

type FixtureType = {
  MovingHead?: String;
  Light?: String;
  Dimmer?: String;
};

export interface FixtureState {
  alpha: 0;
  brightness: 0;
  color: Color;
  start_addr: 42;
  strobe_speed: 0;
}

export interface Color {
  r: number;
  g: number;
  b: number;
}

export interface EngineSelection {
  group_ids: number[];
  fixtures_in_group: number[];
}

//
// Selection
//

export interface GroupSelection {
  onlyFixtures: Map<number, boolean>;
  entireGroup: boolean;
}
