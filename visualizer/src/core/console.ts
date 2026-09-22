/**
 * A small built-in lighting console.
 *
 * The visualiser ships with it for two reasons: it makes the app demonstrable
 * without a running blaulicht instance on the network, and it gives the test
 * suite a realistic traffic source that exercises the full
 * encode -> Art-Net -> receive -> decode path.
 *
 * It is deterministic in `time`, so a test can assert exact channel values.
 */

import { encoderFor } from './encode.js';
import { hsvToRgb } from './color.js';
import { DMX_UNIVERSE_SIZE } from './artnet.js';
import type { FixtureState } from './profiles.js';
import type { Layout, PlacedFixture } from './layout.js';

export const DEMO_PATTERNS = [
  'rainbow-chase',
  'group-sweep',
  'colour-wash',
  'strobe-hits',
  'beam-fan',
  'blackout',
] as const;

export type DemoPattern = (typeof DEMO_PATTERNS)[number];

function baseState(): FixtureState {
  return { r: 0, g: 0, b: 0, alpha: 0, pan: 128, tilt: 128, strobe: 0, focus: 128 };
}

/** Triangle wave in 0..1 with period 1. */
function tri(x: number): number {
  const f = x - Math.floor(x);
  return f < 0.5 ? f * 2 : 2 - f * 2;
}

function stateFor(
  pattern: DemoPattern,
  fixture: PlacedFixture,
  index: number,
  total: number,
  groupIndex: number,
  groupCount: number,
  t: number,
): FixtureState {
  const s = baseState();
  const phase = total <= 1 ? 0 : index / total;

  switch (pattern) {
    case 'rainbow-chase': {
      const hue = ((t * 60 + phase * 720) % 360 + 360) % 360;
      const rgb = hsvToRgb(hue, 1, 1);
      s.r = rgb.r;
      s.g = rgb.g;
      s.b = rgb.b;
      // A bright head travelling along the rig, everything else at a low base.
      const head = tri(t * 0.5 - phase);
      s.alpha = Math.round(40 + 215 * Math.pow(head, 6));
      break;
    }
    case 'group-sweep': {
      // A soft front travelling across the groups. The envelope is wider than
      // one group on purpose: with a narrow one, a rig of 18 groups is fully
      // lit only at isolated instants and mostly looks dead.
      const span = 2.5;
      const active = (t * 0.8) % Math.max(1, groupCount);
      const raw = Math.abs(active - groupIndex);
      const distance = Math.min(raw, groupCount - raw);
      const near = Math.max(0, 1 - distance / span);
      const rgb = hsvToRgb((groupIndex * 47) % 360, 0.9, 1);
      s.r = rgb.r;
      s.g = rgb.g;
      s.b = rgb.b;
      s.alpha = Math.round(255 * near);
      break;
    }
    case 'colour-wash': {
      const hue = ((t * 20 + fixture.world.x * 12) % 360 + 360) % 360;
      const rgb = hsvToRgb(hue, 0.85, 1);
      s.r = rgb.r;
      s.g = rgb.g;
      s.b = rgb.b;
      s.alpha = Math.round(180 + 75 * Math.sin(t * 2 + phase * Math.PI * 2));
      break;
    }
    case 'strobe-hits': {
      s.r = 255;
      s.g = 255;
      s.b = 255;
      const beat = Math.floor(t * 2);
      const hit = (beat + index) % 4 === 0;
      s.alpha = hit ? 255 : 0;
      s.strobe = hit ? 200 : 0;
      break;
    }
    case 'beam-fan': {
      const rgb = hsvToRgb((phase * 360 + t * 30) % 360, 1, 1);
      s.r = rgb.r;
      s.g = rgb.g;
      s.b = rgb.b;
      s.alpha = 255;
      s.pan = Math.round(128 + 110 * Math.sin(t * 1.3 + phase * Math.PI * 2));
      s.tilt = Math.round(128 + 80 * Math.sin(t * 0.7 + phase * Math.PI));
      break;
    }
    case 'blackout':
      break;
  }

  s.alpha = Math.max(0, Math.min(255, Math.round(s.alpha)));
  return s;
}

/**
 * Renders one frame of the demo show.
 *
 * @param time seconds since the show started.
 * @returns one zero-initialised 512-byte buffer per universe in the layout.
 */
export function renderDemoFrame(
  layout: Layout,
  pattern: DemoPattern,
  time: number,
): Map<number, Uint8Array> {
  const universes = new Map<number, Uint8Array>();
  const groupKeys = [...new Set(layout.fixtures.map((f) => f.groupKey))].sort(
    (a, b) => a - b,
  );

  // Per-group indices so a chase reads as travelling along each truss.
  const indexInGroup = new Map<string, number>();
  const groupSize = new Map<number, number>();
  for (const f of layout.fixtures) {
    const n = groupSize.get(f.groupKey) ?? 0;
    indexInGroup.set(f.id, n);
    groupSize.set(f.groupKey, n + 1);
  }

  for (const fixture of layout.fixtures) {
    let buffer = universes.get(fixture.universe);
    if (!buffer) {
      buffer = new Uint8Array(DMX_UNIVERSE_SIZE);
      universes.set(fixture.universe, buffer);
    }

    const encode = encoderFor(fixture.kind, fixture.model);
    if (!encode) continue;

    const state = stateFor(
      pattern,
      fixture,
      indexInGroup.get(fixture.id) ?? 0,
      groupSize.get(fixture.groupKey) ?? 1,
      groupKeys.indexOf(fixture.groupKey),
      groupKeys.length,
      time,
    );
    encode(state, buffer, fixture.base);
  }

  return universes;
}
