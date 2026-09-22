/**
 * Tolerant parser for blaulicht showfiles.
 *
 * The format has drifted: files in this repo carry `format_version` 3 and 4,
 * and older ones have no version field and no `stage` section at all. Groups and
 * fixtures serialise as `BTreeMap`s, which serde may emit either as
 * `[{key, value}, ...]` or as a plain `{ "0": value }` object depending on the
 * encoder. Both shapes are accepted here; anything unparseable becomes a
 * warning rather than a hard failure, so a partially-understood showfile still
 * renders.
 */

import { lookupProfile, unknownProfile, type FixtureProfile } from './profiles.js';

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface ShowFixture {
  /** Stable identity: `group.fixture` key pair from the showfile. */
  id: string;
  groupKey: number;
  groupName: string;
  fixtureKey: number;
  name: string;
  kind: string;
  model: string;
  profile: FixtureProfile;
  universe: number;
  /** 1-based DMX start address as stored in the showfile. */
  startAddr: number;
  /** 0-based offset into the Art-Net payload of this fixture's channel 0. */
  base: number;
  pos: Vec3;
  rotation: Vec3;
}

export interface ShowGroup {
  key: number;
  name: string;
  fixtureIds: string[];
}

export interface Room {
  visible: boolean;
  width: number;
  depth: number;
  height: number;
  floorColor: [number, number, number];
  wallColor: [number, number, number];
}

export interface ArtNetReceiver {
  address: string;
  enabled: boolean;
  firstUniverse: number;
  lastUniverse: number;
}

export interface Show {
  name: string;
  formatVersion: number | null;
  groups: ShowGroup[];
  fixtures: ShowFixture[];
  /** Sorted list of universes referenced by at least one fixture. */
  universes: number[];
  room: Room;
  receivers: ArtNetReceiver[];
  /** True when every fixture sits at the same point, so positions carry no
   *  information and the auto-layout must supply them. */
  positionsDegenerate: boolean;
  warnings: string[];
}

export const DEFAULT_ROOM: Room = {
  visible: true,
  width: 20,
  depth: 20,
  height: 8,
  floorColor: [28, 30, 38],
  wallColor: [38, 41, 50],
};

/** Normalises a serde `BTreeMap` into `[key, value]` pairs. */
function entries(raw: unknown): Array<[number, any]> {
  if (Array.isArray(raw)) {
    return raw.map((item, i) => {
      if (item && typeof item === 'object' && 'key' in item && 'value' in item) {
        return [Number((item as any).key), (item as any).value];
      }
      // Plain array of values: fall back to positional keys.
      return [i, item];
    });
  }
  if (raw && typeof raw === 'object') {
    return Object.entries(raw as Record<string, unknown>).map(([k, v]) => [
      Number(k),
      v,
    ]);
  }
  return [];
}

function num(raw: unknown, fallback: number): number {
  return typeof raw === 'number' && Number.isFinite(raw) ? raw : fallback;
}

function vec3(raw: unknown): Vec3 {
  const o = (raw ?? {}) as Record<string, unknown>;
  return { x: num(o.x, 0), y: num(o.y, 0), z: num(o.z, 0) };
}

function rgbTriple(
  raw: unknown,
  fallback: [number, number, number],
): [number, number, number] {
  if (Array.isArray(raw) && raw.length >= 3) {
    return [num(raw[0], fallback[0]), num(raw[1], fallback[1]), num(raw[2], fallback[2])];
  }
  return fallback;
}

/** Splits `{ "Light": "AdjMegaHexPar" }` into kind and model. Also accepts a
 *  bare string, and `{ kind, model }`, which older tooling emitted. */
function splitType(raw: unknown): { kind: string; model: string } {
  if (typeof raw === 'string') return { kind: 'Light', model: raw };
  if (raw && typeof raw === 'object') {
    const o = raw as Record<string, unknown>;
    if (typeof o.kind === 'string' && typeof o.model === 'string') {
      return { kind: o.kind, model: o.model };
    }
    const keys = Object.keys(o);
    if (keys.length === 1 && typeof o[keys[0]] === 'string') {
      return { kind: keys[0], model: o[keys[0]] as string };
    }
  }
  return { kind: 'Light', model: 'Unknown' };
}

function parseRoom(stage: unknown, warnings: string[]): Room {
  const room = (stage as any)?.room;
  if (!room || typeof room !== 'object') {
    if (stage) warnings.push('Showfile has a stage section but no room; using defaults.');
    return { ...DEFAULT_ROOM };
  }
  return {
    visible: room.visible !== false,
    width: Math.max(1, num(room.width, DEFAULT_ROOM.width)),
    depth: Math.max(1, num(room.depth, DEFAULT_ROOM.depth)),
    height: Math.max(1, num(room.height, DEFAULT_ROOM.height)),
    floorColor: rgbTriple(room.floor_color, DEFAULT_ROOM.floorColor),
    wallColor: rgbTriple(room.wall_color, DEFAULT_ROOM.wallColor),
  };
}

function parseReceivers(artnet: unknown): ArtNetReceiver[] {
  const list = (artnet as any)?.receivers;
  if (!Array.isArray(list)) return [];
  return list.map((r: any) => ({
    address: typeof r?.address === 'string' ? r.address : '0.0.0.0:6454',
    enabled: r?.enabled !== false,
    // Older showfiles omit the universe span; treat them as "all universes".
    firstUniverse: num(r?.first_universe, 0),
    lastUniverse: num(r?.last_universe, 15),
  }));
}

export function parseShowfile(raw: unknown, name = 'showfile'): Show {
  const warnings: string[] = [];

  if (!raw || typeof raw !== 'object') {
    throw new Error('Showfile is not a JSON object.');
  }
  const root = raw as Record<string, any>;
  if (!root.engine || typeof root.engine !== 'object') {
    throw new Error('Showfile has no `engine` section — is this a blaulicht showfile?');
  }

  const groups: ShowGroup[] = [];
  const fixtures: ShowFixture[] = [];
  const universeSet = new Set<number>();

  for (const [groupKey, groupValue] of entries(root.engine.groups)) {
    const groupName =
      typeof groupValue?.name === 'string' ? groupValue.name : `Group ${groupKey}`;
    const fixtureIds: string[] = [];

    for (const [fixtureKey, fx] of entries(groupValue?.fixtures)) {
      if (!fx || typeof fx !== 'object') {
        warnings.push(`Group ${groupKey}: fixture ${fixtureKey} is not an object; skipped.`);
        continue;
      }
      const { kind, model } = splitType(fx.type_ ?? fx.type);
      let profile = lookupProfile(kind, model);
      if (!profile) {
        warnings.push(`Unknown fixture profile "${kind}:${model}" — rendered as a generic dimmer.`);
        profile = unknownProfile(kind, model);
      }

      const startAddr = Math.trunc(num(fx.start_addr, 1));
      const universe = Math.trunc(num(fx.universe_no, 0));
      const id = `${groupKey}.${fixtureKey}`;

      if (startAddr < 1) {
        warnings.push(`${id} has start address ${startAddr}; expected >= 1.`);
      }
      if (startAddr - 1 + profile.footprint > 512) {
        warnings.push(
          `${id} (${model}) runs past the end of universe ${universe}: ` +
            `start ${startAddr} + ${profile.footprint} channels.`,
        );
      }

      universeSet.add(universe);
      fixtureIds.push(id);
      fixtures.push({
        id,
        groupKey,
        groupName,
        fixtureKey,
        name: typeof fx.name === 'string' ? fx.name : `Fixture ${fixtureKey}`,
        kind,
        model,
        profile,
        universe,
        startAddr,
        // blaulicht's DMX buffer is 1-based; Art-Net payload index 0 is DMX ch 1.
        base: startAddr - 1,
        pos: vec3(fx.pos),
        rotation: vec3(fx.rotation),
      });
    }

    groups.push({ key: groupKey, name: groupName, fixtureIds });
  }

  if (fixtures.length === 0) {
    warnings.push('Showfile contains no fixtures.');
  }

  const first = fixtures[0]?.pos;
  const positionsDegenerate =
    fixtures.length > 1 &&
    first !== undefined &&
    fixtures.every(
      (f) => f.pos.x === first.x && f.pos.y === first.y && f.pos.z === first.z,
    );
  if (positionsDegenerate) {
    warnings.push(
      'All fixtures share one position — the showfile carries no stage geometry. ' +
        'Using the generated layout.',
    );
  }

  // Overlap detection: two fixtures whose channel ranges collide in a universe.
  const occupied = new Map<string, string>();
  for (const f of fixtures) {
    for (let c = 0; c < f.profile.footprint; c++) {
      const key = `${f.universe}:${f.base + c}`;
      const prev = occupied.get(key);
      if (prev && prev !== f.id) {
        warnings.push(
          `Patch overlap in universe ${f.universe} at channel ${f.base + c + 1}: ${prev} and ${f.id}.`,
        );
        break;
      }
      occupied.set(key, f.id);
    }
  }

  return {
    name,
    formatVersion:
      typeof root.format_version === 'number' ? root.format_version : null,
    groups,
    fixtures,
    universes: [...universeSet].sort((a, b) => a - b),
    room: parseRoom(root.stage, warnings),
    receivers: parseReceivers(root.artnet),
    positionsDegenerate,
    warnings,
  };
}
