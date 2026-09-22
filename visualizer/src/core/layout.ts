/**
 * Stage layout.
 *
 * Every showfile in this repo stores each fixture at the origin — blaulicht's
 * engine never needed real coordinates — so a visualiser that trusted `pos`
 * would draw 500 lights inside one another. When positions are degenerate we
 * synthesise a rig: one truss row per group, fixtures spread along it, with the
 * row's height and depth chosen from the kind of fixtures it holds.
 */

import type { Show, ShowFixture, Vec3 } from './showfile.js';

export interface PlacedFixture extends ShowFixture {
  /** World-space position actually used for rendering. */
  world: Vec3;
  /** Unit vector the beam points along at pan/tilt centre. */
  aim: Vec3;
  /** Rendered size in metres. */
  scale: number;
}

export interface Layout {
  fixtures: PlacedFixture[];
  byId: Map<string, PlacedFixture>;
  /** Axis-aligned bounds of all placed fixtures, for camera framing. */
  bounds: { min: Vec3; max: Vec3; center: Vec3; radius: number };
  generated: boolean;
}

const MIN_SPACING = 0.22;
const MAX_SPACING = 1.4;

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), hi);
}

/**
 * Trim height for a single fixture.
 *
 * Decided per fixture rather than per group: a hazer sharing a group with the
 * moving heads still belongs on the deck, and a group's majority kind is a poor
 * proxy for where any individual fixture hangs.
 */
function fixtureHeight(fixture: ShowFixture, roomHeight: number): number {
  if (fixture.profile.beam === 'head') return roomHeight * 0.92;
  // Hazers and fog machines are the non-emitting floor devices.
  if (fixture.profile.beam === 'none') return roomHeight * 0.04;
  if (fixture.profile.kind === 'Dimmer') return roomHeight * 0.1;
  return roomHeight * 0.74;
}

function fixtureScale(f: ShowFixture, spacing: number): number {
  const base = f.profile.beam === 'head' ? 0.34 : 0.26;
  // Never draw a fixture wider than its slot, so dense strips stay readable.
  return clamp(Math.min(base, spacing * 0.8), 0.05, 0.45);
}

/** Straight down for overheads, straight up for anything sitting on the floor. */
function aimFor(height: number, roomHeight: number): Vec3 {
  if (height < roomHeight * 0.25) return { x: 0, y: 1, z: 0 };
  return { x: 0, y: -1, z: 0 };
}

function generatedLayout(show: Show): PlacedFixture[] {
  const { width, depth, height } = show.room;
  const groups = show.groups.filter((g) => g.fixtureIds.length > 0);
  const byId = new Map(show.fixtures.map((f) => [f.id, f]));
  const placed: PlacedFixture[] = [];

  groups.forEach((group, gi) => {
    const members = group.fixtureIds
      .map((id) => byId.get(id))
      .filter((f): f is ShowFixture => f !== undefined);
    if (members.length === 0) return;

    const spacing = clamp(width / (members.length + 1), MIN_SPACING, MAX_SPACING);
    const rowWidth = spacing * (members.length - 1);
    // Rows are evenly spread across the room depth, front row nearest the camera.
    const z =
      groups.length === 1
        ? 0
        : -depth / 2 + (depth * (gi + 1)) / (groups.length + 1);

    members.forEach((f, i) => {
      const y = fixtureHeight(f, height);
      placed.push({
        ...f,
        world: { x: -rowWidth / 2 + i * spacing, y, z },
        aim: aimFor(y, height),
        scale: fixtureScale(f, spacing),
      });
    });
  });

  return placed;
}

function explicitLayout(show: Show): PlacedFixture[] {
  return show.fixtures.map((f) => ({
    ...f,
    world: { ...f.pos },
    aim: aimFor(f.pos.y, show.room.height),
    scale: fixtureScale(f, 0.5),
  }));
}

export function buildLayout(show: Show): Layout {
  const generated = show.positionsDegenerate || show.fixtures.length <= 1;
  const fixtures = generated ? generatedLayout(show) : explicitLayout(show);

  const min: Vec3 = { x: Infinity, y: Infinity, z: Infinity };
  const max: Vec3 = { x: -Infinity, y: -Infinity, z: -Infinity };
  for (const f of fixtures) {
    min.x = Math.min(min.x, f.world.x);
    min.y = Math.min(min.y, f.world.y);
    min.z = Math.min(min.z, f.world.z);
    max.x = Math.max(max.x, f.world.x);
    max.y = Math.max(max.y, f.world.y);
    max.z = Math.max(max.z, f.world.z);
  }
  if (fixtures.length === 0) {
    min.x = min.y = min.z = 0;
    max.x = max.y = max.z = 0;
  }

  const center: Vec3 = {
    x: (min.x + max.x) / 2,
    y: (min.y + max.y) / 2,
    z: (min.z + max.z) / 2,
  };
  const radius = Math.max(
    1,
    Math.hypot(max.x - min.x, max.y - min.y, max.z - min.z) / 2,
  );

  return {
    fixtures,
    byId: new Map(fixtures.map((f) => [f.id, f])),
    bounds: { min, max, center, radius },
    generated,
  };
}
