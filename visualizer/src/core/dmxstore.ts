/**
 * Live DMX state: one 512-byte buffer per universe, fed by Art-Net packets.
 *
 * Kept independent of Node so the same store can run in the main process (fed
 * by a UDP socket) and in tests (fed by hand-built packets).
 */

import {
  DMX_UNIVERSE_SIZE,
  isNewerSequence,
  type ArtDmxPacket,
  type ArtNetRejection,
} from './artnet.js';

export interface UniverseStats {
  universe: number;
  /** Packets accepted into the buffer. */
  packets: number;
  /** Packets dropped because their sequence number was stale. */
  outOfOrder: number;
  lastSequence: number;
  /** `performance.now()`-style timestamp of the last accepted packet. */
  lastUpdate: number;
  /** Accepted packets per second, averaged over the last second. */
  rate: number;
}

export interface StoreStats {
  universes: UniverseStats[];
  totalPackets: number;
  rejected: Record<ArtNetRejection | 'stale', number>;
}

const EMPTY_REJECTIONS = (): Record<ArtNetRejection | 'stale', number> => ({
  'too-short': 0,
  'bad-id': 0,
  'unsupported-opcode': 0,
  'bad-protocol-version': 0,
  'bad-length': 0,
  truncated: 0,
  stale: 0,
});

interface UniverseSlot {
  data: Uint8Array;
  packets: number;
  outOfOrder: number;
  lastSequence: number;
  lastUpdate: number;
  /** Timestamps of recent accepted packets, trimmed to a 1s window. */
  recent: number[];
  dirty: boolean;
}

export class DmxStore {
  private readonly slots = new Map<number, UniverseSlot>();
  private readonly rejections = EMPTY_REJECTIONS();
  private accepted = 0;

  /** Universes the store will track even before any packet arrives, so the HUD
   *  can show a patched-but-silent universe. */
  constructor(preallocate: Iterable<number> = []) {
    for (const universe of preallocate) this.slot(universe);
  }

  private slot(universe: number): UniverseSlot {
    let slot = this.slots.get(universe);
    if (!slot) {
      slot = {
        data: new Uint8Array(DMX_UNIVERSE_SIZE),
        packets: 0,
        outOfOrder: 0,
        lastSequence: 0,
        lastUpdate: 0,
        recent: [],
        dirty: true,
      };
      this.slots.set(universe, slot);
    }
    return slot;
  }

  /** Applies a DMX packet. Returns false when it was dropped as out of order. */
  apply(packet: ArtDmxPacket, now: number): boolean {
    const slot = this.slot(packet.universe);

    if (slot.packets > 0 && !isNewerSequence(slot.lastSequence, packet.sequence)) {
      slot.outOfOrder++;
      this.rejections.stale++;
      return false;
    }

    // A short packet only refreshes the channels it carries; the rest of the
    // universe keeps its previous values, which is what Art-Net specifies.
    slot.data.set(packet.data.subarray(0, DMX_UNIVERSE_SIZE), 0);
    slot.lastSequence = packet.sequence;
    slot.lastUpdate = now;
    slot.packets++;
    slot.dirty = true;
    slot.recent.push(now);
    this.accepted++;
    return true;
  }

  recordRejection(reason: ArtNetRejection): void {
    this.rejections[reason]++;
  }

  get(universe: number): Uint8Array | undefined {
    return this.slots.get(universe)?.data;
  }

  /** Buffer for `universe`, allocating a zeroed one if it has never been seen.
   *  Lets the renderer address every patched universe unconditionally. */
  getOrCreate(universe: number): Uint8Array {
    return this.slot(universe).data;
  }

  has(universe: number): boolean {
    return this.slots.has(universe);
  }

  universes(): number[] {
    return [...this.slots.keys()].sort((a, b) => a - b);
  }

  /** Universes changed since the last `clearDirty()`. */
  dirty(): number[] {
    const out: number[] = [];
    for (const [universe, slot] of this.slots) if (slot.dirty) out.push(universe);
    return out.sort((a, b) => a - b);
  }

  clearDirty(): void {
    for (const slot of this.slots.values()) slot.dirty = false;
  }

  stats(now: number): StoreStats {
    const universes: UniverseStats[] = [];
    for (const [universe, slot] of this.slots) {
      // Trim the rate window before measuring it.
      const cutoff = now - 1000;
      while (slot.recent.length > 0 && slot.recent[0] < cutoff) slot.recent.shift();
      universes.push({
        universe,
        packets: slot.packets,
        outOfOrder: slot.outOfOrder,
        lastSequence: slot.lastSequence,
        lastUpdate: slot.lastUpdate,
        rate: slot.recent.length,
      });
    }
    universes.sort((a, b) => a.universe - b.universe);
    return {
      universes,
      totalPackets: this.accepted,
      rejected: { ...this.rejections },
    };
  }
}
