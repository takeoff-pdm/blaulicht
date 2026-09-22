/**
 * Showfile parsing, including every real blaulicht showfile in this repo.
 */

import { existsSync, readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { DEFAULT_ROOM, parseShowfile } from '../src/core/showfile.js';

const REPO = resolve(import.meta.dirname, '../..');
const MINI = JSON.parse(
  readFileSync(resolve(import.meta.dirname, 'fixtures/mini-show.json'), 'utf8'),
);

describe('parseShowfile', () => {
  it('reads groups, fixtures, patch and room from a well-formed showfile', () => {
    const show = parseShowfile(MINI, 'mini-show.json');

    expect(show.formatVersion).toBe(4);
    expect(show.groups.map((g) => g.name)).toEqual(['Front Truss', 'Heads']);
    expect(show.fixtures).toHaveLength(4);
    expect(show.universes).toEqual([0, 1]);
    expect(show.room).toMatchObject({ width: 14, depth: 10, height: 6 });
    expect(show.receivers).toEqual([
      { address: '127.0.0.1:6454', enabled: true, firstUniverse: 0, lastUniverse: 15 },
    ]);
  });

  it('converts the 1-based DMX start address into an Art-Net payload offset', () => {
    const show = parseShowfile(MINI);
    const par = show.fixtures.find((f) => f.name === 'Par L')!;
    // blaulicht's internal buffer drops slot 0 before transmitting, so DMX
    // channel 1 is payload index 0.
    expect(par.startAddr).toBe(1);
    expect(par.base).toBe(0);

    const mac = show.fixtures.find((f) => f.name === 'Mac 1')!;
    expect(mac.startAddr).toBe(100);
    expect(mac.base).toBe(99);
  });

  it('resolves each fixture to its profile', () => {
    const show = parseShowfile(MINI);
    const mac = show.fixtures.find((f) => f.name === 'Mac 1')!;
    expect(mac.kind).toBe('MovingHead');
    expect(mac.profile.footprint).toBe(18);
    expect(mac.profile.beam).toBe('head');

    const haze = show.fixtures.find((f) => f.name === 'Haze')!;
    expect(haze.profile.beam).toBe('none');
  });

  it('notices that all fixtures share one position', () => {
    const show = parseShowfile(MINI);
    expect(show.positionsDegenerate).toBe(true);
    expect(show.warnings.join('\n')).toMatch(/generated layout/i);
  });

  it('keeps real positions when the showfile actually has them', () => {
    const withPositions = structuredClone(MINI);
    withPositions.engine.groups[0].value.fixtures[0].value.pos = { x: -3, y: 5, z: 1 };
    const show = parseShowfile(withPositions);
    expect(show.positionsDegenerate).toBe(false);
    expect(show.fixtures[0].pos).toEqual({ x: -3, y: 5, z: 1 });
  });

  describe('tolerates format drift', () => {
    it('accepts a showfile with no format_version and no stage section', () => {
      const legacy = structuredClone(MINI);
      delete legacy.format_version;
      delete legacy.stage;
      const show = parseShowfile(legacy);
      expect(show.formatVersion).toBeNull();
      expect(show.room).toEqual(DEFAULT_ROOM);
      expect(show.fixtures).toHaveLength(4);
    });

    it('accepts BTreeMaps serialised as plain objects instead of key/value arrays', () => {
      const asObjects = {
        engine: {
          groups: {
            '0': {
              name: 'Obj Group',
              fixtures: {
                '7': {
                  name: 'Par', type_: { Light: 'KuzeLEDPar' },
                  pos: { x: 0, y: 0, z: 0 }, start_addr: 5, universe_no: 2,
                },
              },
            },
          },
        },
      };
      const show = parseShowfile(asObjects);
      expect(show.fixtures).toHaveLength(1);
      expect(show.fixtures[0].id).toBe('0.7');
      expect(show.fixtures[0].universe).toBe(2);
      expect(show.universes).toEqual([2]);
    });

    it('falls back to a generic profile for an unknown fixture model', () => {
      const unknown = structuredClone(MINI);
      unknown.engine.groups[0].value.fixtures[0].value.type_ = { Light: 'FutureLaser9000' };
      const show = parseShowfile(unknown);
      expect(show.fixtures[0].model).toBe('FutureLaser9000');
      expect(show.fixtures[0].profile.footprint).toBe(1);
      expect(show.warnings.join('\n')).toMatch(/Unknown fixture profile/);
    });

    it('defaults a missing rotation to zero', () => {
      const noRotation = structuredClone(MINI);
      delete noRotation.engine.groups[0].value.fixtures[0].value.rotation;
      expect(parseShowfile(noRotation).fixtures[0].rotation).toEqual({ x: 0, y: 0, z: 0 });
    });
  });

  describe('rejects input it cannot use', () => {
    it('refuses a non-object', () => {
      expect(() => parseShowfile('nope')).toThrow(/not a JSON object/);
    });
    it('refuses JSON that is not a showfile', () => {
      expect(() => parseShowfile({ hello: 'world' })).toThrow(/no `engine` section/);
    });
  });

  describe('patch validation', () => {
    it('warns when two fixtures overlap in the same universe', () => {
      const clash = structuredClone(MINI);
      // AdjMegaHexPar occupies 7 channels; starting the second at 4 collides.
      clash.engine.groups[0].value.fixtures[1].value.start_addr = 4;
      const show = parseShowfile(clash);
      expect(show.warnings.join('\n')).toMatch(/Patch overlap in universe 0/);
    });

    it('warns when a fixture runs past the end of its universe', () => {
      const overrun = structuredClone(MINI);
      overrun.engine.groups[1].value.fixtures[0].value.start_addr = 500;
      const show = parseShowfile(overrun);
      expect(show.warnings.join('\n')).toMatch(/runs past the end of universe 1/);
    });
  });
});

describe('real blaulicht showfiles in this repo', () => {
  const candidates = [
    'SPARTACUS_DRAFT.json',
    '17.01.json',
    'Kuze_Theater.migrated.json',
    'Kuze_migrated.json',
    'crates/core/OUTDOOR.json',
  ]
    .map((p) => resolve(REPO, p))
    .filter(existsSync);

  it('finds at least one showfile to check against', () => {
    expect(candidates.length).toBeGreaterThan(0);
  });

  for (const path of candidates) {
    it(`parses ${basename(path)}`, () => {
      const show = parseShowfile(JSON.parse(readFileSync(path, 'utf8')), basename(path));

      expect(show.fixtures.length).toBeGreaterThan(0);
      expect(show.universes.length).toBeGreaterThan(0);

      for (const fixture of show.fixtures) {
        expect(fixture.profile.footprint).toBeGreaterThan(0);
        expect(fixture.base).toBeGreaterThanOrEqual(0);
        expect(fixture.universe).toBeGreaterThanOrEqual(0);
        // Every fixture must resolve to a real profile, not the fallback.
        expect(
          fixture.profile.channels[0],
          `${basename(path)}: ${fixture.model} fell back to the unknown profile`,
        ).not.toBe('Unknown');
      }
    });
  }
});
