/**
 * DMX encoders: the inverse of `src/core/profiles.ts`, ported from the `write`
 * implementations in `crates/shared/src/fixture/*.rs`.
 *
 * These drive the built-in demo console (so the visualiser is useful without an
 * external lighting desk) and let the test-suite assert that a state survives
 * an encode -> Art-Net -> decode round-trip, which is the property that actually
 * matters for fidelity against blaulicht.
 */

import { mapRangeU8 } from './color.js';
import type { FixtureState } from './profiles.js';

export type Encoder = (state: FixtureState, data: Uint8Array, base: number) => void;

function put(data: Uint8Array, base: number, index: number, value: number): void {
  const i = base + index;
  if (i < 0 || i >= data.length) return;
  data[i] = Math.max(0, Math.min(255, Math.round(value)));
}

/** Colour after the master dimmer, matching the pre-multiplication that the
 *  Rust writer does for profiles without a dimmer channel. */
export function effectiveRgb(state: FixtureState): { r: number; g: number; b: number } {
  const a = state.alpha / 255;
  return {
    r: Math.trunc(state.r * a),
    g: Math.trunc(state.g * a),
    b: Math.trunc(state.b * a),
  };
}

/** RGB -> hue in degrees, matching the hue-only profiles' wire format. */
function hueOf(state: FixtureState): number {
  const r = state.r / 255;
  const g = state.g / 255;
  const b = state.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  if (d === 0) return 0;
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h *= 60;
  return h < 0 ? h + 360 : h;
}

function satOf(state: FixtureState): number {
  const max = Math.max(state.r, state.g, state.b);
  const min = Math.min(state.r, state.g, state.b);
  return max === 0 ? 0 : (max - min) / max;
}

const MAC250E_WHEEL: Array<[number, number | null]> = [
  [157, null],
  [185, 0.0],
  [197, 30.0],
  [165, 55.0],
  [177, 125.0],
  [201, 150.0],
  [193, 200.0],
  [181, 220.0],
  [169, 240.0],
  [205, 275.0],
  [189, 305.0],
  [173, 335.0],
];

function hueDistance(a: number, b: number): number {
  const d = ((a - b) % 360 + 360) % 360;
  return Math.min(d, 360 - d);
}

function mac250eNearestWheel(state: FixtureState): number {
  const v = Math.max(state.r, state.g, state.b) / 255;
  if (satOf(state) < 0.15 || v < 0.05) return 157;
  const hue = hueOf(state);
  let best = 157;
  let bestDist = Infinity;
  for (const [dmx, h] of MAC250E_WHEEL) {
    if (h === null) continue;
    const dist = hueDistance(h, hue);
    if (dist < bestDist) {
      bestDist = dist;
      best = dmx;
    }
  }
  return best;
}

function mac250eStrobeToDmx(speed: number): number {
  if (speed === 0) return 30; // shutter open
  return 72 - mapRangeU8(speed, [1, 255], [0, 22]);
}

/** Several profiles emit full white on a dedicated channel only when the
 *  colour is exactly 255/255/255, mirroring the Rust writer. */
function whiteOverride(s: FixtureState): number {
  return s.r === 255 && s.g === 255 && s.b === 255 ? 255 : 0;
}

const LIGHT_ENCODERS: Record<string, Encoder> = {
  Generic3ChanNoAlpha: (s, d, b) => {
    const c = effectiveRgb(s);
    put(d, b, 0, c.r);
    put(d, b, 1, c.g);
    put(d, b, 2, c.b);
  },
  GenericColorAlphaLight: (s, d, b) => {
    put(d, b, 0, (hueOf(s) / 360) * 255);
    put(d, b, 1, s.alpha);
  },
  Generic4ChanWithAlpha: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.r);
    put(d, b, 2, s.g);
    put(d, b, 3, s.b);
  },
  LEDPartyTCLSpot: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 3, s.alpha);
  },
  AdjMegaHexPar: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 6, s.alpha);
  },
  LiteCraftMiniParAT10: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 7, s.alpha);
  },
  VaryTechVP1: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.strobe);
    put(d, b, 2, 127);
    put(d, b, 3, 127);
  },
  LightMaxxVegaSilentPar2Quad: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.strobe);
    put(d, b, 2, 0);
    put(d, b, 3, 0);
    put(d, b, 4, s.r);
    put(d, b, 5, s.g);
    put(d, b, 6, s.b);
    put(d, b, 7, whiteOverride(s));
  },
  LEDPar64RGBSpot5Chan: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 3, s.alpha);
    put(d, b, 4, s.strobe === 0 ? 0 : mapRangeU8(s.strobe, [0, 255], [11, 255]));
  },
  CameoQSpot40RGBW_4Chan: (s, d, b) => {
    const c = effectiveRgb(s);
    put(d, b, 0, c.r);
    put(d, b, 1, c.g);
    put(d, b, 2, c.b);
    put(d, b, 3, whiteOverride(s));
  },
  LightMaxxTripleDerbyHP: (s, d, b) => {
    put(d, b, 0, (hueOf(s) / 360) * 255);
    put(d, b, 1, s.pan);
    put(d, b, 2, s.strobe);
    put(d, b, 3, s.alpha);
  },
  EuroLiteLEDMultiFX_10Chan: (s, d, b) => {
    const hue = (hueOf(s) / 360) * 255;
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.alpha);
    put(d, b, 2, s.strobe);
    put(d, b, 3, hue);
    put(d, b, 4, hue);
    put(d, b, 5, s.pan);
    put(d, b, 6, s.strobe);
    put(d, b, 7, s.tilt);
    put(d, b, 8, s.focus);
    put(d, b, 9, s.pan);
  },
  TakeOffLogo: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.r);
    put(d, b, 2, s.g);
    put(d, b, 3, s.b);
    put(d, b, 4, s.focus);
    put(d, b, 5, s.focus);
  },
  KuzeLEDPar: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 4, 0);
    put(d, b, 5, s.strobe);
    put(d, b, 6, 0);
    put(d, b, 7, s.alpha);
  },
  StairvilleWildWash648RGB_6Chan: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.strobe);
    put(d, b, 2, s.r);
    put(d, b, 3, s.g);
    put(d, b, 4, s.b);
    put(d, b, 5, 0);
  },
  StairvilleLEDPar56_7Chan: (s, d, b) => {
    put(d, b, 0, s.r);
    put(d, b, 1, s.g);
    put(d, b, 2, s.b);
    put(d, b, 3, 0);
    put(d, b, 4, s.strobe === 0 ? 0 : mapRangeU8(s.strobe, [0, 255], [16, 255]));
    put(d, b, 5, 0);
    put(d, b, 6, s.alpha);
  },
};

const MOVING_HEAD_ENCODERS: Record<string, Encoder> = {
  MartinMac250E: (s, d, b) => {
    put(d, b, 0, mac250eStrobeToDmx(s.strobe));
    put(d, b, 1, s.alpha);
    put(d, b, 2, 0);
    put(d, b, 3, mac250eNearestWheel(s));
    for (const c of [4, 5, 6, 7, 8]) put(d, b, c, 0);
    put(d, b, 9, s.focus);
    put(d, b, 10, 0);
    put(d, b, 11, 0);
    put(d, b, 12, s.pan);
    put(d, b, 13, 0);
    put(d, b, 14, s.tilt);
    for (const c of [15, 16, 17]) put(d, b, c, 0);
  },
  VaryTechHeroSpot60: (s, d, b) => {
    put(d, b, 0, s.pan);
    put(d, b, 1, s.tilt);
    put(d, b, 2, 0);
    put(d, b, 3, s.alpha);
    put(d, b, 4, s.strobe === 0 ? 0 : mapRangeU8(s.strobe, [1, 255], [10, 250]));
    put(d, b, 5, s.focus);
    put(d, b, 6, satOf(s) * 255);
  },
};

const DIMMER_ENCODERS: Record<string, Encoder> = {
  FogMachineSingle: (s, d, b) => put(d, b, 0, s.alpha),
  DimmerSingle: (s, d, b) => put(d, b, 0, s.alpha),
  DimmerWStrobe: (s, d, b) => {
    put(d, b, 0, s.alpha);
    put(d, b, 1, s.strobe);
  },
};

const ENCODERS: Record<string, Record<string, Encoder>> = {
  Light: LIGHT_ENCODERS,
  MovingHead: MOVING_HEAD_ENCODERS,
  Dimmer: DIMMER_ENCODERS,
};

export function encoderFor(kind: string, model: string): Encoder | undefined {
  return ENCODERS[kind]?.[model];
}
