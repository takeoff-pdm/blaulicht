import { describe, expect, it } from 'vitest';

import { buildArtDmxPacket, parseArtNetPacket, type ArtDmxPacket } from '../src/core/artnet.js';
import { DmxStore } from '../src/core/dmxstore.js';

function packet(universe: number, sequence: number, data: number[]): ArtDmxPacket {
  const payload = new Uint8Array(512);
  payload.set(data);
  return parseArtNetPacket(
    buildArtDmxPacket({ universe, data: payload, sequence }),
  ) as ArtDmxPacket;
}

describe('DmxStore', () => {
  it('writes channel data into the addressed universe', () => {
    const store = new DmxStore();
    expect(store.apply(packet(2, 1, [10, 20, 30]), 0)).toBe(true);

    expect([...store.get(2)!.subarray(0, 3)]).toEqual([10, 20, 30]);
    expect(store.get(1)).toBeUndefined();
    expect(store.universes()).toEqual([2]);
  });

  it('drops packets whose sequence has gone backwards', () => {
    const store = new DmxStore();
    store.apply(packet(0, 10, [1]), 0);
    expect(store.apply(packet(0, 9, [99]), 1)).toBe(false);

    expect(store.get(0)![0]).toBe(1);
    expect(store.stats(1).universes[0].outOfOrder).toBe(1);
    expect(store.stats(1).rejected.stale).toBe(1);
  });

  it('follows the sequence counter across its 255 -> 1 wrap', () => {
    const store = new DmxStore();
    store.apply(packet(0, 254, [1]), 0);
    expect(store.apply(packet(0, 255, [2]), 1)).toBe(true);
    expect(store.apply(packet(0, 1, [3]), 2)).toBe(true);
    expect(store.get(0)![0]).toBe(3);
  });

  it('preallocates patched universes so a silent one is still listed', () => {
    const store = new DmxStore([0, 1, 4]);
    expect(store.universes()).toEqual([0, 1, 4]);
    expect(store.get(4)).toHaveLength(512);
    expect(store.stats(0).universes.map((u) => u.packets)).toEqual([0, 0, 0]);
  });

  it('tracks which universes changed since the last flush', () => {
    const store = new DmxStore();
    store.apply(packet(3, 1, [1]), 0);
    store.apply(packet(5, 1, [1]), 0);
    expect(store.dirty()).toEqual([3, 5]);

    store.clearDirty();
    expect(store.dirty()).toEqual([]);

    store.apply(packet(5, 2, [2]), 1);
    expect(store.dirty()).toEqual([5]);
  });

  it('measures packet rate over a one-second window', () => {
    const store = new DmxStore();
    for (let i = 0; i < 44; i++) store.apply(packet(0, i + 1, [i]), 1000 + i * 20);

    // All 44 packets land inside the window ending at t = 1880.
    expect(store.stats(1880).universes[0].rate).toBe(44);
    // A second later the window has emptied.
    expect(store.stats(3000).universes[0].rate).toBe(0);
  });

  it('counts protocol-level rejections separately from stale packets', () => {
    const store = new DmxStore();
    store.recordRejection('bad-id');
    store.recordRejection('bad-id');
    store.recordRejection('truncated');

    const stats = store.stats(0);
    expect(stats.rejected['bad-id']).toBe(2);
    expect(stats.rejected.truncated).toBe(1);
    expect(stats.rejected.stale).toBe(0);
    expect(stats.totalPackets).toBe(0);
  });
});
