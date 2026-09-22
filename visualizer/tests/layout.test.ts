import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { buildLayout } from '../src/core/layout.js';
import { parseShowfile } from '../src/core/showfile.js';

const MINI = JSON.parse(
  readFileSync(resolve(import.meta.dirname, 'fixtures/mini-show.json'), 'utf8'),
);

function distinctPositions(fixtures: Array<{ world: { x: number; y: number; z: number } }>) {
  return new Set(fixtures.map((f) => `${f.world.x.toFixed(4)},${f.world.y.toFixed(4)},${f.world.z.toFixed(4)}`));
}

describe('buildLayout', () => {
  it('generates a rig when the showfile has no usable positions', () => {
    const layout = buildLayout(parseShowfile(MINI));
    expect(layout.generated).toBe(true);
    // Four fixtures stacked at the origin must become four distinct places.
    expect(distinctPositions(layout.fixtures).size).toBe(4);
  });

  it('puts each group on its own row in depth', () => {
    const layout = buildLayout(parseShowfile(MINI));
    const zByGroup = new Map<number, number>();
    for (const f of layout.fixtures) zByGroup.set(f.groupKey, f.world.z);
    expect(zByGroup.size).toBe(2);
    expect(new Set(zByGroup.values()).size).toBe(2);
  });

  it('hangs heads high and leaves floor packages low', () => {
    const layout = buildLayout(parseShowfile(MINI));
    const mac = layout.byId.get('1.0')!; // MartinMac250E, a beam fixture
    const par = layout.byId.get('0.0')!; // AdjMegaHexPar, a wash
    expect(mac.world.y).toBeGreaterThan(par.world.y);
    expect(mac.aim).toEqual({ x: 0, y: -1, z: 0 });
  });

  it('honours real coordinates when the showfile has them', () => {
    const positioned = structuredClone(MINI);
    const fixtures = positioned.engine.groups[0].value.fixtures;
    fixtures[0].value.pos = { x: -4, y: 5, z: 2 };
    fixtures[1].value.pos = { x: 4, y: 5, z: 2 };

    const layout = buildLayout(parseShowfile(positioned));
    expect(layout.generated).toBe(false);
    expect(layout.byId.get('0.0')!.world).toEqual({ x: -4, y: 5, z: 2 });
    expect(layout.byId.get('0.1')!.world).toEqual({ x: 4, y: 5, z: 2 });
  });

  it('reports bounds that enclose every fixture', () => {
    const layout = buildLayout(parseShowfile(MINI));
    for (const f of layout.fixtures) {
      expect(f.world.x).toBeGreaterThanOrEqual(layout.bounds.min.x);
      expect(f.world.x).toBeLessThanOrEqual(layout.bounds.max.x);
      expect(f.world.y).toBeGreaterThanOrEqual(layout.bounds.min.y);
      expect(f.world.y).toBeLessThanOrEqual(layout.bounds.max.y);
      expect(f.world.z).toBeGreaterThanOrEqual(layout.bounds.min.z);
      expect(f.world.z).toBeLessThanOrEqual(layout.bounds.max.z);
    }
    expect(layout.bounds.radius).toBeGreaterThan(0);
    expect(Number.isFinite(layout.bounds.center.x)).toBe(true);
  });

  it('keeps dense rows from overlapping by shrinking the fixtures', () => {
    // 120 fixtures on one 14 m truss: spacing collapses, so scale must too.
    const dense = structuredClone(MINI);
    dense.engine.groups = [dense.engine.groups[0]];
    dense.engine.groups[0].value.fixtures = Array.from({ length: 120 }, (_, i) => ({
      key: i,
      value: {
        name: `Cell ${i}`,
        type_: { Light: 'Generic3ChanNoAlpha' },
        pos: { x: 0, y: 0, z: 0 },
        rotation: { x: 0, y: 0, z: 0 },
        start_addr: 1 + i * 3,
        universe_no: 0,
      },
    }));

    const layout = buildLayout(parseShowfile(dense));
    expect(layout.fixtures).toHaveLength(120);
    expect(distinctPositions(layout.fixtures).size).toBe(120);

    const sorted = [...layout.fixtures].sort((a, b) => a.world.x - b.world.x);
    const spacing = sorted[1].world.x - sorted[0].world.x;
    for (const f of layout.fixtures) {
      expect(f.scale).toBeLessThanOrEqual(spacing);
      expect(f.scale).toBeGreaterThan(0);
    }
  });

  it('survives an empty showfile without producing NaN bounds', () => {
    const layout = buildLayout(parseShowfile({ engine: { groups: [] } }));
    expect(layout.fixtures).toHaveLength(0);
    expect(Number.isFinite(layout.bounds.center.x)).toBe(true);
    expect(Number.isFinite(layout.bounds.radius)).toBe(true);
  });
});
