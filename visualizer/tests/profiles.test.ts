/**
 * Fixture-profile fidelity.
 *
 * The visualiser is only worth looking at if its decoders agree with
 * blaulicht's. These tests pin three things: the channel footprints, the
 * golden DMX vectors lifted from the Rust unit tests, and an
 * encode -> decode round-trip for every profile the app ships.
 */

import { describe, expect, it } from 'vitest';

import { PROFILES, lookupProfile, type FixtureState } from '../src/core/profiles.js';
import { effectiveRgb, encoderFor } from '../src/core/encode.js';

/** Footprints as declared in `crates/shared/src/fixture/*.rs`. */
const FOOTPRINTS: Record<string, number> = {
  Generic3ChanNoAlpha: 3,
  GenericColorAlphaLight: 2,
  Generic4ChanWithAlpha: 4,
  LEDPartyTCLSpot: 6,
  AdjMegaHexPar: 7,
  LiteCraftMiniParAT10: 8,
  VaryTechVP1: 4,
  LightMaxxVegaSilentPar2Quad: 8,
  LEDPar64RGBSpot5Chan: 5,
  CameoQSpot40RGBW_4Chan: 4,
  LightMaxxTripleDerbyHP: 4,
  EuroLiteLEDMultiFX_10Chan: 10,
  TakeOffLogo: 6,
  KuzeLEDPar: 8,
  StairvilleWildWash648RGB_6Chan: 6,
  StairvilleLEDPar56_7Chan: 7,
  MartinMac250E: 18,
  VaryTechHeroSpot60: 8,
  FogMachineSingle: 1,
  DimmerSingle: 1,
  DimmerWStrobe: 2,
};

function allProfiles(): Array<{ kind: string; model: string }> {
  const out: Array<{ kind: string; model: string }> = [];
  for (const [kind, table] of Object.entries(PROFILES)) {
    for (const model of Object.keys(table)) out.push({ kind, model });
  }
  return out;
}

function hueOf(c: { r: number; g: number; b: number }): number {
  const r = c.r / 255;
  const g = c.g / 255;
  const b = c.b / 255;
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  if (d === 0) return 0;
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h *= 60;
  return h < 0 ? h + 360 : h;
}

function hueDistance(a: number, b: number): number {
  const d = Math.abs(a - b) % 360;
  return Math.min(d, 360 - d);
}

describe('profile registry', () => {
  it('declares the same channel footprints as the Rust fixture definitions', () => {
    const seen = new Set<string>();
    for (const { kind, model } of allProfiles()) {
      const profile = lookupProfile(kind, model)!;
      expect(FOOTPRINTS[model], `${model} missing from the reference table`).toBeDefined();
      expect(profile.footprint, `${kind}:${model} footprint`).toBe(FOOTPRINTS[model]);
      seen.add(model);
    }
    // The reference table must not drift ahead of the implementation either.
    expect([...seen].sort()).toEqual(Object.keys(FOOTPRINTS).sort());
  });

  it('documents one channel name per channel', () => {
    for (const { kind, model } of allProfiles()) {
      const profile = lookupProfile(kind, model)!;
      expect(profile.channels, `${model} channel labels`).toHaveLength(profile.footprint);
    }
  });

  it('ships an encoder for every profile', () => {
    for (const { kind, model } of allProfiles()) {
      expect(encoderFor(kind, model), `${kind}:${model}`).toBeTypeOf('function');
    }
  });
});

describe('golden vectors from the Rust test-suite', () => {
  // Ported from `mac250e_shutter_open_and_effects_off` in moving_head.rs. The
  // Rust buffer is 1-based with the fixture at start address 400, so passing
  // base = 400 into a 513-byte array reproduces its indices exactly.
  it('encodes a MAC 250 Entour the way blaulicht does', () => {
    const encode = encoderFor('MovingHead', 'MartinMac250E')!;
    const dmx = new Uint8Array(513);
    const state: FixtureState = {
      // h = 5 deg, s = 1, v = 1 -> the "Red 301" wheel slot.
      r: 255, g: 21, b: 0,
      alpha: 255,
      pan: 10,
      tilt: 200,
      strobe: 0,
      focus: 77,
    };
    encode(state, dmx, 400);

    expect(dmx[400], 'shutter must sit in the open range').toBeGreaterThanOrEqual(20);
    expect(dmx[400]).toBeLessThanOrEqual(49);
    expect(dmx[401]).toBe(255);
    expect(dmx[403]).toBe(185); // Red 301
    expect(dmx[405]).toBe(0); // rotating gobo open
    expect(dmx[408]).toBe(0); // static gobo open
    expect(dmx[409]).toBe(77); // focus
    expect(dmx[411]).toBe(0); // prism off
    expect(dmx[412]).toBe(10); // pan
    expect(dmx[414]).toBe(200); // tilt
    expect(dmx[418], 'must not bleed into the next head').toBe(0);

    const back = lookupProfile('MovingHead', 'MartinMac250E')!.decode(dmx, 400);
    expect(back.alpha).toBe(255);
    expect(back.strobe).toBe(0);
    expect(back.pan).toBe(10);
    expect(back.tilt).toBe(200);
    expect(back.focus).toBe(77);
    expect(Math.trunc(hueOf(back))).toBe(0);
  });

  // Ported from `mac250e_strobe_stays_in_strobe_range`.
  it('keeps the MAC 250 shutter inside the strobe band and never on a reset code', () => {
    const encode = encoderFor('MovingHead', 'MartinMac250E')!;
    const profile = lookupProfile('MovingHead', 'MartinMac250E')!;
    const dmx = new Uint8Array(513);

    for (const strobe of [1, 128, 255]) {
      encode({ r: 0, g: 0, b: 0, alpha: 0, pan: 0, tilt: 0, strobe, focus: 0 }, dmx, 400);
      expect(dmx[400], `strobe ${strobe}`).toBeGreaterThanOrEqual(50);
      expect(dmx[400], `strobe ${strobe}`).toBeLessThanOrEqual(72);
    }

    // Fastest and slowest pin the ends of the band exactly.
    encode({ r: 0, g: 0, b: 0, alpha: 0, pan: 0, tilt: 0, strobe: 255, focus: 0 }, dmx, 400);
    expect(dmx[400]).toBe(50);
    expect(profile.decode(dmx, 400).strobe).toBe(255);

    encode({ r: 0, g: 0, b: 0, alpha: 0, pan: 0, tilt: 0, strobe: 1, focus: 0 }, dmx, 400);
    expect(dmx[400]).toBe(72);
    expect(profile.decode(dmx, 400).strobe).toBe(1);

    // An open shutter decodes as "no strobe".
    dmx[400] = 30;
    expect(profile.decode(dmx, 400).strobe).toBe(0);
  });

  it('pre-multiplies colour by the dimmer on profiles without a dimmer channel', () => {
    // Generic3ChanNoAlpha has no master dimmer, so blaulicht scales RGB itself.
    const encode = encoderFor('Light', 'Generic3ChanNoAlpha')!;
    const dmx = new Uint8Array(512);
    encode({ r: 200, g: 100, b: 50, alpha: 128, pan: 0, tilt: 0, strobe: 0, focus: 0 }, dmx, 0);

    expect([dmx[0], dmx[1], dmx[2]]).toEqual([100, 50, 25]);
    const back = lookupProfile('Light', 'Generic3ChanNoAlpha')!.decode(dmx, 0);
    expect(back).toMatchObject({ r: 100, g: 50, b: 25, alpha: 255 });
  });
});

describe('visualiser-specific colour handling', () => {
  it('renders a white blinder as white, not black', () => {
    // blaulicht's own decoder reports BLACK for the VP1 because its engine only
    // drives the dimmer. Taking that literally made a blinder at full output
    // invisible on stage, so the visualiser derives white from the fixed
    // warm/cold white channels instead.
    const encode = encoderFor('Light', 'VaryTechVP1')!;
    const profile = lookupProfile('Light', 'VaryTechVP1')!;
    const dmx = new Uint8Array(512);
    encode(
      { r: 0, g: 0, b: 0, alpha: 255, pan: 0, tilt: 0, strobe: 0, focus: 0 },
      dmx,
      0,
    );

    const back = profile.decode(dmx, 0);
    expect(back.alpha).toBe(255);
    expect(Math.min(back.r, back.g, back.b)).toBeGreaterThan(120);
    // Warm and cold white are driven equally, so the result must be near-neutral.
    expect(Math.max(back.r, back.g, back.b) - Math.min(back.r, back.g, back.b))
      .toBeLessThan(90);
  });

  it('gives a bare dimmer channel a tungsten colour so it is visible', () => {
    const profile = lookupProfile('Dimmer', 'DimmerSingle')!;
    const dmx = new Uint8Array(512);
    dmx[0] = 255;
    const back = profile.decode(dmx, 0);
    expect(back.alpha).toBe(255);
    expect(back.r).toBeGreaterThan(back.b); // warm
    expect(back.r + back.g + back.b).toBeGreaterThan(300);
  });

  it('keeps a fog machine non-emitting', () => {
    expect(lookupProfile('Dimmer', 'FogMachineSingle')!.beam).toBe('none');
  });
});

describe('encode -> decode round-trip', () => {
  /** What each profile can actually carry on the wire. */
  interface Carries {
    rgb?: boolean;
    /** Colour survives only after the dimmer is folded in. */
    effectiveRgb?: boolean;
    hue?: boolean;
    alpha?: boolean;
    pan?: boolean;
    tilt?: boolean;
    focus?: boolean;
    /** Tolerance for lossy strobe range remapping; omitted = no strobe channel. */
    strobeTolerance?: number;
  }

  const CARRIES: Record<string, Carries> = {
    Generic3ChanNoAlpha: { effectiveRgb: true },
    GenericColorAlphaLight: { hue: true, alpha: true },
    Generic4ChanWithAlpha: { rgb: true, alpha: true },
    LEDPartyTCLSpot: { rgb: true, alpha: true },
    AdjMegaHexPar: { rgb: true, alpha: true },
    LiteCraftMiniParAT10: { rgb: true, alpha: true },
    VaryTechVP1: { alpha: true, strobeTolerance: 0 },
    LightMaxxVegaSilentPar2Quad: { rgb: true, alpha: true, strobeTolerance: 0 },
    LEDPar64RGBSpot5Chan: { rgb: true, alpha: true, strobeTolerance: 3 },
    CameoQSpot40RGBW_4Chan: { effectiveRgb: true },
    LightMaxxTripleDerbyHP: { hue: true, pan: true, alpha: true, strobeTolerance: 0 },
    EuroLiteLEDMultiFX_10Chan: {
      hue: true, alpha: true, pan: true, tilt: true, focus: true, strobeTolerance: 0,
    },
    TakeOffLogo: { rgb: true, alpha: true, focus: true },
    KuzeLEDPar: { rgb: true, alpha: true, strobeTolerance: 0 },
    StairvilleWildWash648RGB_6Chan: { rgb: true, alpha: true, strobeTolerance: 0 },
    StairvilleLEDPar56_7Chan: { rgb: true, alpha: true, strobeTolerance: 3 },
    // The wheel quantises colour to 12 slots, so only hue-adjacency is testable.
    MartinMac250E: { alpha: true, pan: true, tilt: true, focus: true, strobeTolerance: 14 },
    VaryTechHeroSpot60: { pan: true, tilt: true, alpha: true, focus: true, strobeTolerance: 3 },
    FogMachineSingle: { alpha: true },
    DimmerSingle: { alpha: true },
    DimmerWStrobe: { alpha: true, strobeTolerance: 0 },
  };

  const SOURCE: FixtureState = {
    r: 0, g: 180, b: 220, // a distinctly hued, non-grey colour
    alpha: 176,
    pan: 61,
    tilt: 203,
    strobe: 137,
    focus: 92,
  };

  for (const { kind, model } of allProfiles()) {
    it(`${kind}:${model}`, () => {
      const profile = lookupProfile(kind, model)!;
      const encode = encoderFor(kind, model)!;
      const carries = CARRIES[model];
      expect(carries, `${model} has no round-trip expectation`).toBeDefined();

      // Offset the fixture inside the universe to catch base-address mistakes.
      const base = 40;
      const dmx = new Uint8Array(512);
      encode(SOURCE, dmx, base);
      const back = profile.decode(dmx, base);

      if (carries.rgb) {
        expect({ r: back.r, g: back.g, b: back.b }).toEqual({
          r: SOURCE.r, g: SOURCE.g, b: SOURCE.b,
        });
      }
      if (carries.effectiveRgb) {
        expect({ r: back.r, g: back.g, b: back.b }).toEqual(effectiveRgb(SOURCE));
        expect(back.alpha).toBe(255);
      }
      if (carries.hue) {
        // Hue travels as a single byte, so expect quantisation, not equality.
        expect(hueDistance(hueOf(back), hueOf(SOURCE))).toBeLessThanOrEqual(3);
      }
      if (carries.alpha) expect(back.alpha).toBe(SOURCE.alpha);
      if (carries.pan) expect(back.pan).toBe(SOURCE.pan);
      if (carries.tilt) expect(back.tilt).toBe(SOURCE.tilt);
      if (carries.focus) expect(back.focus).toBe(SOURCE.focus);
      if (carries.strobeTolerance !== undefined) {
        expect(Math.abs(back.strobe - SOURCE.strobe)).toBeLessThanOrEqual(
          carries.strobeTolerance,
        );
      } else {
        expect(back.strobe).toBe(0);
      }
    });
  }

  it('never writes outside a fixture’s own channel range', () => {
    for (const { kind, model } of allProfiles()) {
      const profile = lookupProfile(kind, model)!;
      const encode = encoderFor(kind, model)!;
      const base = 100;
      const dmx = new Uint8Array(512).fill(0xaa);
      encode(SOURCE, dmx, base);

      for (let i = 0; i < dmx.length; i++) {
        if (i >= base && i < base + profile.footprint) continue;
        expect(dmx[i], `${model} touched channel ${i + 1}`).toBe(0xaa);
      }
    }
  });

  it('reads zero rather than throwing when a fixture is patched past the universe end', () => {
    const profile = lookupProfile('Light', 'EuroLiteLEDMultiFX_10Chan')!;
    const dmx = new Uint8Array(512);
    // Start address 510 leaves only three channels inside the universe.
    expect(() => profile.decode(dmx, 509)).not.toThrow();
    expect(profile.decode(dmx, 509).alpha).toBe(0);
  });
});
