/**
 * Fixture profiles: a faithful port of `state_from_dmx` from
 * `crates/shared/src/fixture/{light,moving_head,dimmer}.rs`.
 *
 * Channel indexing: blaulicht's internal DMX buffer is 1-based (slot 0 is
 * dropped before the Art-Net payload is built, see `crates/core/src/dmx/mod.rs`).
 * Here `base` is the 0-based Art-Net offset of the fixture's channel 0, i.e.
 * `base = start_addr - 1`.
 */

import { hsvToRgb, mapRangeU8, type RGB } from './color.js';

export type FixtureKind = 'Light' | 'MovingHead' | 'Dimmer';

/** How the fixture should be drawn: a projecting head, a soft wash, or a
 *  non-emitting device (dimmer channel, hazer). */
export type BeamShape = 'head' | 'wash' | 'none';

export interface FixtureState {
  /** Base colour before the master dimmer is applied, 0..255. */
  r: number;
  g: number;
  b: number;
  /** Master dimmer, 0..255. */
  alpha: number;
  /** Raw pan/tilt DMX, 0..255. */
  pan: number;
  tilt: number;
  /** Normalised strobe rate, 0 = off. */
  strobe: number;
  focus: number;
}

export interface FixtureProfile {
  kind: FixtureKind;
  model: string;
  footprint: number;
  beam: BeamShape;
  /** Human-readable channel layout, shown in the inspector. */
  channels: string[];
  decode(data: Uint8Array, base: number): FixtureState;
}

const BLACK: Readonly<FixtureState> = {
  r: 0,
  g: 0,
  b: 0,
  alpha: 0,
  pan: 0,
  tilt: 0,
  strobe: 0,
  focus: 0,
};

function state(partial: Partial<FixtureState>): FixtureState {
  return { ...BLACK, ...partial };
}

/** Reads one channel, returning 0 for reads past the end of the universe so a
 *  mis-patched fixture degrades instead of throwing mid-frame. */
function ch(data: Uint8Array, base: number, index: number): number {
  const i = base + index;
  return i >= 0 && i < data.length ? data[i] : 0;
}

function rgbAt(data: Uint8Array, base: number, offset = 0): RGB {
  return {
    r: ch(data, base, offset),
    g: ch(data, base, offset + 1),
    b: ch(data, base, offset + 2),
  };
}

//
// Martin MAC 250 Entour colour wheel (see moving_head.rs).
//
const MAC250E_WHEEL: Array<[number, number | null]> = [
  [157, null], // White
  [185, 0.0], // Red 301
  [197, 30.0], // Orange 306
  [165, 55.0], // Yellow 603
  [177, 125.0], // Green 206
  [201, 150.0], // Dark green
  [193, 200.0], // Blue 101 (light)
  [181, 220.0], // Blue 108
  [169, 240.0], // Blue 104
  [205, 275.0], // Purple 502
  [189, 305.0], // Magenta 507
  [173, 335.0], // Pink 312
];

function mac250eWheelColorFromDmx(dmx: number): RGB {
  let best = MAC250E_WHEEL[0];
  let bestDist = Infinity;
  for (const entry of MAC250E_WHEEL) {
    const dist = Math.abs(entry[0] - dmx);
    if (dist < bestDist) {
      bestDist = dist;
      best = entry;
    }
  }
  const hue = best[1];
  return hue === null ? { r: 255, g: 255, b: 255 } : hsvToRgb(hue, 1, 1);
}

/** Shutter channel 50..72 is strobe fast -> slow; anything else is open. */
function mac250eStrobeFromDmx(dmx: number): number {
  if (dmx >= 50 && dmx <= 72) return mapRangeU8(72 - dmx, [0, 22], [1, 255]);
  return 0;
}

function profile(
  kind: FixtureKind,
  model: string,
  footprint: number,
  beam: BeamShape,
  channels: string[],
  decode: (data: Uint8Array, base: number) => FixtureState,
): FixtureProfile {
  return { kind, model, footprint, beam, channels, decode };
}

//
// Lights.
//
export const LIGHT_PROFILES: Record<string, FixtureProfile> = {
  Generic3ChanNoAlpha: profile(
    'Light',
    'Generic3ChanNoAlpha',
    3,
    'wash',
    ['Red', 'Green', 'Blue'],
    // The writer pre-multiplies RGB by alpha, so the wire colour is final.
    (d, b) => state({ ...rgbAt(d, b), alpha: 255 }),
  ),

  GenericColorAlphaLight: profile(
    'Light',
    'GenericColorAlphaLight',
    2,
    'wash',
    ['Hue', 'Dimmer'],
    (d, b) =>
      state({
        ...hsvToRgb((ch(d, b, 0) / 255) * 360, 1, 1),
        alpha: ch(d, b, 1),
      }),
  ),

  Generic4ChanWithAlpha: profile(
    'Light',
    'Generic4ChanWithAlpha',
    4,
    'wash',
    ['Dimmer', 'Red', 'Green', 'Blue'],
    (d, b) => state({ ...rgbAt(d, b, 1), alpha: ch(d, b, 0) }),
  ),

  LEDPartyTCLSpot: profile(
    'Light',
    'LEDPartyTCLSpot',
    6,
    'wash',
    ['Red', 'Green', 'Blue', 'Dimmer', '-', 'Strobe'],
    (d, b) => state({ ...rgbAt(d, b), alpha: ch(d, b, 3) }),
  ),

  AdjMegaHexPar: profile(
    'Light',
    'AdjMegaHexPar',
    7,
    'wash',
    ['Red', 'Green', 'Blue', '-', '-', '-', 'Dimmer'],
    (d, b) => state({ ...rgbAt(d, b), alpha: ch(d, b, 6) }),
  ),

  LiteCraftMiniParAT10: profile(
    'Light',
    'LiteCraftMiniParAT10',
    8,
    'wash',
    ['Red', 'Green', 'Blue', '-', '-', '-', '-', 'Dimmer'],
    (d, b) => state({ ...rgbAt(d, b), alpha: ch(d, b, 7) }),
  ),

  VaryTechVP1: profile(
    'Light',
    'VaryTechVP1',
    4,
    'wash',
    ['Dimmer', 'Strobe', 'Warm White', 'Cold White'],
    // Deliberately differs from the Rust decoder, which reports BLACK here
    // because blaulicht's engine has no use for this fixture's colour — it only
    // ever drives the dimmer. A visualiser that took that literally would draw
    // a blinder at full output as invisible. The writer pins both the warm and
    // cold white channels to 127, so the emitted light is a neutral white; that
    // is what gets rendered.
    (d, b) => {
      const warm = ch(d, b, 2);
      const cold = ch(d, b, 3);
      const total = warm + cold;
      // Blend the two white sources: warm 2700 K-ish, cold 6500 K-ish.
      const mix = total === 0 ? 0.5 : cold / total;
      return state({
        r: Math.round(255 - 5 * mix),
        g: Math.round(200 + 40 * mix),
        b: Math.round(150 + 105 * mix),
        alpha: ch(d, b, 0),
        strobe: ch(d, b, 1),
      });
    },
  ),

  LightMaxxVegaSilentPar2Quad: profile(
    'Light',
    'LightMaxxVegaSilentPar2Quad',
    8,
    'wash',
    ['Dimmer', 'Strobe', 'Macro', 'Macro Speed', 'Red', 'Green', 'Blue', 'White'],
    (d, b) =>
      state({
        ...rgbAt(d, b, 4),
        alpha: ch(d, b, 0),
        strobe: ch(d, b, 1),
      }),
  ),

  LEDPar64RGBSpot5Chan: profile(
    'Light',
    'LEDPar64RGBSpot5Chan',
    5,
    'wash',
    ['Red', 'Green', 'Blue', 'Dimmer', 'Strobe'],
    (d, b) => {
      const raw = ch(d, b, 4);
      return state({
        ...rgbAt(d, b),
        alpha: ch(d, b, 3),
        strobe: raw < 11 ? 0 : mapRangeU8(raw, [11, 255], [0, 255]),
      });
    },
  ),

  CameoQSpot40RGBW_4Chan: profile(
    'Light',
    'CameoQSpot40RGBW_4Chan',
    4,
    'wash',
    ['Red', 'Green', 'Blue', 'White'],
    (d, b) => {
      if (ch(d, b, 3) === 255) {
        const r = ch(d, b, 0);
        const g = ch(d, b, 1);
        const bl = ch(d, b, 2);
        return state({ r: 255, g: 255, b: 255, alpha: Math.max(r, g, bl) });
      }
      return state({ ...rgbAt(d, b), alpha: 255 });
    },
  ),

  LightMaxxTripleDerbyHP: profile(
    'Light',
    'LightMaxxTripleDerbyHP',
    4,
    'head',
    ['Hue', 'Rotation', 'Strobe', 'Dimmer'],
    (d, b) =>
      state({
        ...hsvToRgb((ch(d, b, 0) / 255) * 360, 1, 1),
        pan: ch(d, b, 1),
        strobe: ch(d, b, 2),
        alpha: ch(d, b, 3),
      }),
  ),

  EuroLiteLEDMultiFX_10Chan: profile(
    'Light',
    'EuroLiteLEDMultiFX_10Chan',
    10,
    'head',
    [
      'UV Dimmer',
      'UV Dimmer',
      'Strobe',
      'Matrix Hue',
      'Speed',
      'Laser Hue',
      'Laser Strobe',
      'Laser Rotation',
      'White SMDs',
      'Speed',
    ],
    (d, b) =>
      state({
        ...hsvToRgb((ch(d, b, 3) / 255) * 360, 1, 1),
        alpha: ch(d, b, 0),
        strobe: ch(d, b, 2),
        pan: ch(d, b, 5),
        tilt: ch(d, b, 7),
        focus: ch(d, b, 8),
      }),
  ),

  TakeOffLogo: profile(
    'Light',
    'TakeOffLogo',
    6,
    'wash',
    ['Dimmer', 'Red', 'Green', 'Blue', 'BPM', 'Focus'],
    (d, b) =>
      state({ ...rgbAt(d, b, 1), alpha: ch(d, b, 0), focus: ch(d, b, 5) }),
  ),

  KuzeLEDPar: profile(
    'Light',
    'KuzeLEDPar',
    8,
    'wash',
    ['Red', 'Green', 'Blue', '-', 'Macro', 'Strobe', 'Chase', 'Dimmer'],
    (d, b) =>
      state({ ...rgbAt(d, b), strobe: ch(d, b, 5), alpha: ch(d, b, 7) }),
  ),

  StairvilleWildWash648RGB_6Chan: profile(
    'Light',
    'StairvilleWildWash648RGB_6Chan',
    6,
    'wash',
    ['Dimmer', 'Strobe', 'Red', 'Green', 'Blue', 'Sound'],
    (d, b) =>
      state({
        ...rgbAt(d, b, 2),
        alpha: ch(d, b, 0),
        strobe: ch(d, b, 1),
      }),
  ),

  StairvilleLEDPar56_7Chan: profile(
    'Light',
    'StairvilleLEDPar56_7Chan',
    7,
    'wash',
    ['Red', 'Green', 'Blue', 'Macro', 'Strobe', 'Mode', 'Dimmer'],
    (d, b) => {
      const raw = ch(d, b, 4);
      return state({
        ...rgbAt(d, b),
        strobe: raw < 16 ? 0 : mapRangeU8(raw, [16, 255], [0, 255]),
        alpha: ch(d, b, 6),
      });
    },
  ),
};

//
// Moving heads.
//
export const MOVING_HEAD_PROFILES: Record<string, FixtureProfile> = {
  MartinMac250E: profile(
    'MovingHead',
    'MartinMac250E',
    18,
    'head',
    [
      'Shutter',
      'Dimmer',
      'Dimmer fine',
      'Colour wheel',
      'Colour fine',
      'Rot. gobo',
      'Gobo rot.',
      'Gobo rot. fine',
      'Static gobo',
      'Focus',
      'Focus fine',
      'Prism',
      'Pan',
      'Pan fine',
      'Tilt',
      'Tilt fine',
      'P/T speed',
      'FX speed',
    ],
    (d, b) =>
      state({
        ...mac250eWheelColorFromDmx(ch(d, b, 3)),
        alpha: ch(d, b, 1),
        pan: ch(d, b, 12),
        tilt: ch(d, b, 14),
        strobe: mac250eStrobeFromDmx(ch(d, b, 0)),
        focus: ch(d, b, 9),
      }),
  ),

  VaryTechHeroSpot60: profile(
    'MovingHead',
    'VaryTechHeroSpot60',
    8,
    'head',
    ['Pan', 'Tilt', 'P/T speed', 'Dimmer', 'Strobe', 'Focus', 'Saturation', 'Gobo'],
    (d, b) => {
      const raw = ch(d, b, 4);
      // Profile only carries saturation on the wire; hue is fixed at 0 (red).
      const sat = ch(d, b, 6) / 255;
      return state({
        ...hsvToRgb(0, sat, 1),
        pan: ch(d, b, 0),
        tilt: ch(d, b, 1),
        alpha: ch(d, b, 3),
        strobe: raw < 10 ? 0 : mapRangeU8(raw, [10, 250], [1, 255]),
        focus: ch(d, b, 5),
      });
    },
  ),
};

//
// Dimmers.
//
export const DIMMER_PROFILES: Record<string, FixtureProfile> = {
  FogMachineSingle: profile('Dimmer', 'FogMachineSingle', 1, 'none', ['Output'], (d, b) =>
    state({ alpha: ch(d, b, 0) }),
  ),
  DimmerSingle: profile('Dimmer', 'DimmerSingle', 1, 'wash', ['Dimmer'], (d, b) =>
    // A plain dimmer channel drives a tungsten-ish lamp: warm white at full.
    state({ r: 255, g: 214, b: 170, alpha: ch(d, b, 0) }),
  ),
  DimmerWStrobe: profile(
    'Dimmer',
    'DimmerWStrobe',
    2,
    'wash',
    ['Dimmer', 'Strobe'],
    (d, b) =>
      state({ r: 255, g: 214, b: 170, alpha: ch(d, b, 0), strobe: ch(d, b, 1) }),
  ),
};

export const PROFILES: Record<FixtureKind, Record<string, FixtureProfile>> = {
  Light: LIGHT_PROFILES,
  MovingHead: MOVING_HEAD_PROFILES,
  Dimmer: DIMMER_PROFILES,
};

/** Looks a profile up by the showfile's `{ "Light": "AdjMegaHexPar" }` shape. */
export function lookupProfile(
  kind: string,
  model: string,
): FixtureProfile | undefined {
  const table = PROFILES[kind as FixtureKind];
  return table ? table[model] : undefined;
}

/** Fallback for showfiles that reference a profile this build does not know.
 *  Treated as a 1-channel dimmer so the fixture still appears and reacts. */
export function unknownProfile(kind: string, model: string): FixtureProfile {
  return profile(
    (kind as FixtureKind) ?? 'Light',
    model,
    1,
    'wash',
    ['Unknown'],
    (d, b) => state({ r: 255, g: 255, b: 255, alpha: ch(d, b, 0) }),
  );
}
