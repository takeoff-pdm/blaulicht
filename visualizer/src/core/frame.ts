/**
 * Binary framing for main -> renderer DMX transport.
 *
 * At 44 Hz across a dozen universes, shipping plain objects over IPC means
 * megabytes of structured-clone traffic per second. Instead each tick is packed
 * into one transferable ArrayBuffer:
 *
 *   u32           universe count
 *   u16 * count   universe numbers
 *   pad           to a 4-byte boundary
 *   u8 * 512 * n  channel data, one block per universe, in the same order
 */

import { DMX_UNIVERSE_SIZE } from './artnet.js';

export interface FrameEntry {
  universe: number;
  data: Uint8Array;
}

function headerBytes(count: number): number {
  return 4 + Math.ceil((count * 2) / 4) * 4;
}

export function encodeFrame(entries: FrameEntry[]): ArrayBuffer {
  const count = entries.length;
  const header = headerBytes(count);
  const buffer = new ArrayBuffer(header + count * DMX_UNIVERSE_SIZE);
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);

  view.setUint32(0, count, true);
  entries.forEach((entry, i) => {
    view.setUint16(4 + i * 2, entry.universe, true);
    bytes.set(
      entry.data.subarray(0, DMX_UNIVERSE_SIZE),
      header + i * DMX_UNIVERSE_SIZE,
    );
  });

  return buffer;
}

export function decodeFrame(buffer: ArrayBuffer): FrameEntry[] {
  if (buffer.byteLength < 4) return [];
  const view = new DataView(buffer);
  const count = view.getUint32(0, true);
  const header = headerBytes(count);

  if (buffer.byteLength < header + count * DMX_UNIVERSE_SIZE) {
    throw new Error(
      `Frame declares ${count} universes but is only ${buffer.byteLength} bytes.`,
    );
  }

  const bytes = new Uint8Array(buffer);
  const entries: FrameEntry[] = [];
  for (let i = 0; i < count; i++) {
    const offset = header + i * DMX_UNIVERSE_SIZE;
    entries.push({
      universe: view.getUint16(4 + i * 2, true),
      data: bytes.subarray(offset, offset + DMX_UNIVERSE_SIZE),
    });
  }
  return entries;
}
