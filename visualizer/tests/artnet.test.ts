import { describe, expect, it } from 'vitest';

import {
  ARTDMX_HEADER_LENGTH,
  ArtNetParseError,
  buildArtDmxPacket,
  isNewerSequence,
  OpCode,
  parseArtNetPacket,
} from '../src/core/artnet.js';

function dmx(fill = 0, length = 512): Uint8Array {
  return new Uint8Array(length).fill(fill);
}

describe('Art-Net codec', () => {
  it('round-trips an ArtDmx packet', () => {
    const data = dmx();
    data[0] = 255;
    data[511] = 7;

    const packet = parseArtNetPacket(
      buildArtDmxPacket({ universe: 3, data, sequence: 42, physical: 1 }),
    );

    expect(packet).toMatchObject({ type: 'dmx', universe: 3, sequence: 42, physical: 1 });
    const parsed = packet as Extract<typeof packet, { type: 'dmx' }>;
    expect(parsed.data).toHaveLength(512);
    expect(parsed.data[0]).toBe(255);
    expect(parsed.data[511]).toBe(7);
  });

  it('splits a 15-bit port address into net and sub-universe', () => {
    const packet = parseArtNetPacket(buildArtDmxPacket({ universe: 0x0205, data: dmx() }));
    expect(packet).toMatchObject({ type: 'dmx', universe: 0x0205, net: 2, subUni: 5 });
  });

  it('lays the header out exactly as the spec requires', () => {
    const buf = buildArtDmxPacket({ universe: 1, data: dmx(0, 4), sequence: 9 });

    expect(Buffer.from(buf.subarray(0, 8)).toString('latin1')).toBe('Art-Net\0');
    // OpCode is little-endian, protocol version big-endian.
    expect(buf[8] | (buf[9] << 8)).toBe(OpCode.Dmx);
    expect((buf[10] << 8) | buf[11]).toBe(14);
    expect(buf[12]).toBe(9);
    expect(buf[14]).toBe(1); // SubUni
    expect(buf[15]).toBe(0); // Net
    expect((buf[16] << 8) | buf[17]).toBe(4); // length, big-endian
    expect(buf.length).toBe(ARTDMX_HEADER_LENGTH + 4);
  });

  it('accepts a short packet and reports only the channels it carries', () => {
    const parsed = parseArtNetPacket(buildArtDmxPacket({ universe: 0, data: dmx(9, 16) }));
    expect((parsed as any).data).toHaveLength(16);
  });

  it('recognises ArtSync and ArtPoll, and ignores ArtPollReply', () => {
    const make = (opcode: number) => {
      const buf = buildArtDmxPacket({ universe: 0, data: dmx(0, 2) });
      buf[8] = opcode & 0xff;
      buf[9] = (opcode >> 8) & 0xff;
      return buf;
    };
    expect(parseArtNetPacket(make(OpCode.Sync))).toEqual({ type: 'sync' });
    expect(parseArtNetPacket(make(OpCode.Poll))).toEqual({ type: 'poll' });
    expect(parseArtNetPacket(make(OpCode.PollReply))).toBeNull();
  });

  describe('rejects malformed input', () => {
    const cases: Array<[string, () => Uint8Array, string]> = [
      ['a runt datagram', () => new Uint8Array(4), 'too-short'],
      ['a foreign protocol', () => new Uint8Array(32).fill(0x41), 'bad-id'],
      [
        'an unknown opcode',
        () => {
          const buf = buildArtDmxPacket({ universe: 0, data: dmx(0, 2) });
          buf[8] = 0x99;
          buf[9] = 0x99;
          return buf;
        },
        'unsupported-opcode',
      ],
      [
        'an obsolete protocol version',
        () => buildArtDmxPacket({ universe: 0, data: dmx(0, 2), protocolVersion: 13 }),
        'bad-protocol-version',
      ],
      [
        'a length field out of range',
        () => buildArtDmxPacket({ universe: 0, data: dmx(0, 2), declaredLength: 9999 }),
        'bad-length',
      ],
      [
        'a payload shorter than declared',
        () => buildArtDmxPacket({ universe: 0, data: dmx(0, 8), declaredLength: 512 }),
        'truncated',
      ],
    ];

    for (const [name, build, reason] of cases) {
      it(name, () => {
        try {
          parseArtNetPacket(build());
          throw new Error('expected a parse error');
        } catch (err) {
          expect(err).toBeInstanceOf(ArtNetParseError);
          expect((err as ArtNetParseError).reason).toBe(reason);
        }
      });
    }
  });

  describe('sequence numbering', () => {
    it('accepts a straightforward increment', () => {
      expect(isNewerSequence(10, 11)).toBe(true);
    });
    it('rejects a replayed or reordered packet', () => {
      expect(isNewerSequence(11, 10)).toBe(false);
      expect(isNewerSequence(11, 11)).toBe(false);
    });
    it('handles the 255 -> 1 wrap', () => {
      expect(isNewerSequence(255, 1)).toBe(true);
      expect(isNewerSequence(250, 3)).toBe(true);
    });
    it('treats sequence 0 as "sequencing disabled"', () => {
      expect(isNewerSequence(200, 0)).toBe(true);
      expect(isNewerSequence(0, 200)).toBe(true);
    });
  });
});
