import { describe, expect, it } from 'vitest';

import { decodeFrame, encodeFrame } from '../src/core/frame.js';
import { DMX_UNIVERSE_SIZE } from '../src/core/artnet.js';

function universe(seed: number): Uint8Array {
  const data = new Uint8Array(DMX_UNIVERSE_SIZE);
  for (let i = 0; i < data.length; i++) data[i] = (i * 7 + seed) & 0xff;
  return data;
}

describe('IPC frame codec', () => {
  it('round-trips a single universe', () => {
    const entries = decodeFrame(encodeFrame([{ universe: 5, data: universe(1) }]));
    expect(entries).toHaveLength(1);
    expect(entries[0].universe).toBe(5);
    expect([...entries[0].data]).toEqual([...universe(1)]);
  });

  it('keeps universes in order and intact across odd counts', () => {
    // Odd counts exercise the 4-byte alignment padding after the id table.
    for (const count of [1, 2, 3, 7, 16]) {
      const input = Array.from({ length: count }, (_, i) => ({
        universe: i * 3,
        data: universe(i),
      }));
      const output = decodeFrame(encodeFrame(input));
      expect(output.map((e) => e.universe)).toEqual(input.map((e) => e.universe));
      output.forEach((entry, i) => {
        expect([...entry.data]).toEqual([...input[i].data]);
      });
    }
  });

  it('handles an empty frame', () => {
    expect(decodeFrame(encodeFrame([]))).toEqual([]);
  });

  it('carries the full 15-bit port address range', () => {
    const entries = decodeFrame(encodeFrame([{ universe: 32767, data: universe(3) }]));
    expect(entries[0].universe).toBe(32767);
  });

  it('pads a short universe buffer rather than corrupting the frame', () => {
    const entries = decodeFrame(encodeFrame([{ universe: 0, data: new Uint8Array([1, 2, 3]) }]));
    expect(entries[0].data).toHaveLength(DMX_UNIVERSE_SIZE);
    expect([...entries[0].data.subarray(0, 4)]).toEqual([1, 2, 3, 0]);
  });

  it('rejects a truncated frame instead of reading past the end', () => {
    const buffer = encodeFrame([{ universe: 0, data: universe(0) }]);
    expect(() => decodeFrame(buffer.slice(0, 100))).toThrow(/declares 1 universes/);
  });
});
