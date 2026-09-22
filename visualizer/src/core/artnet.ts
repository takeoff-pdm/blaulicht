/**
 * Art-Net 4 packet codec (ArtDmx / ArtSync / ArtPoll), kept free of Node and
 * Electron imports so it can be unit-tested and reused on either side of the
 * IPC boundary.
 *
 * Wire layout of ArtDmx:
 *   0..7   "Art-Net\0"
 *   8..9   OpCode, little-endian (0x5000 = OpOutput/ArtDmx)
 *   10..11 Protocol version, big-endian (14)
 *   12     Sequence (0 = disabled)
 *   13     Physical (informational)
 *   14     SubUni  (universe low byte)
 *   15     Net     (universe high byte)
 *   16..17 Data length, big-endian (2..512)
 *   18..   DMX data
 */

export const ARTNET_PORT = 6454;
export const ARTNET_ID = 'Art-Net\0';
export const ARTDMX_HEADER_LENGTH = 18;
export const DMX_UNIVERSE_SIZE = 512;

export const OpCode = {
  Poll: 0x2000,
  PollReply: 0x2100,
  Dmx: 0x5000,
  Nzs: 0x5100,
  Sync: 0x5200,
} as const;

export type OpCodeValue = (typeof OpCode)[keyof typeof OpCode];

export interface ArtDmxPacket {
  type: 'dmx';
  /** 15-bit port address: `net << 8 | subUni`. */
  universe: number;
  net: number;
  subUni: number;
  sequence: number;
  physical: number;
  data: Uint8Array;
}

export interface ArtSyncPacket {
  type: 'sync';
}

export interface ArtPollPacket {
  type: 'poll';
}

export type ArtNetPacket = ArtDmxPacket | ArtSyncPacket | ArtPollPacket;

/** Why a datagram was rejected. Surfaced in the HUD so a mis-configured sender
 *  is visible rather than silently dropped. */
export type ArtNetRejection =
  | 'too-short'
  | 'bad-id'
  | 'unsupported-opcode'
  | 'bad-protocol-version'
  | 'bad-length'
  | 'truncated';

export class ArtNetParseError extends Error {
  constructor(readonly reason: ArtNetRejection, message: string) {
    super(message);
    this.name = 'ArtNetParseError';
  }
}

const ID_BYTES = Uint8Array.from(ARTNET_ID, (c) => c.charCodeAt(0));

function hasArtNetId(buf: Uint8Array): boolean {
  for (let i = 0; i < ID_BYTES.length; i++) {
    if (buf[i] !== ID_BYTES[i]) return false;
  }
  return true;
}

/**
 * Parses one datagram. Returns `null` for packets that are valid Art-Net but
 * carry no DMX we care about (e.g. PollReply); throws `ArtNetParseError` for
 * malformed input.
 */
export function parseArtNetPacket(buf: Uint8Array): ArtNetPacket | null {
  if (buf.length < 12) {
    throw new ArtNetParseError('too-short', `Datagram of ${buf.length} bytes is too short for Art-Net.`);
  }
  if (!hasArtNetId(buf)) {
    throw new ArtNetParseError('bad-id', 'Datagram does not start with the Art-Net identifier.');
  }

  const opcode = buf[8] | (buf[9] << 8);

  if (opcode === OpCode.Sync) return { type: 'sync' };
  if (opcode === OpCode.Poll) return { type: 'poll' };
  if (opcode === OpCode.PollReply) return null;
  if (opcode !== OpCode.Dmx && opcode !== OpCode.Nzs) {
    throw new ArtNetParseError(
      'unsupported-opcode',
      `Unsupported Art-Net opcode 0x${opcode.toString(16).padStart(4, '0')}.`,
    );
  }

  if (buf.length < ARTDMX_HEADER_LENGTH) {
    throw new ArtNetParseError('too-short', `ArtDmx packet of ${buf.length} bytes is missing its header.`);
  }

  const protocolVersion = (buf[10] << 8) | buf[11];
  if (protocolVersion < 14) {
    throw new ArtNetParseError(
      'bad-protocol-version',
      `Art-Net protocol version ${protocolVersion} is older than the required 14.`,
    );
  }

  const length = (buf[16] << 8) | buf[17];
  if (length < 1 || length > DMX_UNIVERSE_SIZE) {
    throw new ArtNetParseError('bad-length', `ArtDmx declares ${length} data bytes; expected 1..512.`);
  }
  if (buf.length < ARTDMX_HEADER_LENGTH + length) {
    throw new ArtNetParseError(
      'truncated',
      `ArtDmx declares ${length} data bytes but only ${buf.length - ARTDMX_HEADER_LENGTH} are present.`,
    );
  }

  const subUni = buf[14];
  const net = buf[15];

  return {
    type: 'dmx',
    universe: (net << 8) | subUni,
    net,
    subUni,
    sequence: buf[12],
    physical: buf[13],
    // Copy: the caller keeps this past the lifetime of the socket's buffer.
    data: buf.slice(ARTDMX_HEADER_LENGTH, ARTDMX_HEADER_LENGTH + length),
  };
}

export interface BuildArtDmxOptions {
  universe: number;
  data: Uint8Array;
  sequence?: number;
  physical?: number;
  /** Overrides the length field — only useful for testing malformed senders. */
  declaredLength?: number;
  protocolVersion?: number;
}

/** Builds an ArtDmx datagram. Used by the renderer's self-test generator and by
 *  the test-suite's synthetic lighting console. */
export function buildArtDmxPacket(opts: BuildArtDmxOptions): Uint8Array {
  const { universe, data } = opts;
  const declared = opts.declaredLength ?? data.length;
  const protocolVersion = opts.protocolVersion ?? 14;
  const buf = new Uint8Array(ARTDMX_HEADER_LENGTH + data.length);

  buf.set(ID_BYTES, 0);
  buf[8] = OpCode.Dmx & 0xff;
  buf[9] = (OpCode.Dmx >> 8) & 0xff;
  buf[10] = (protocolVersion >> 8) & 0xff;
  buf[11] = protocolVersion & 0xff;
  buf[12] = (opts.sequence ?? 0) & 0xff;
  buf[13] = (opts.physical ?? 0) & 0xff;
  buf[14] = universe & 0xff;
  buf[15] = (universe >> 8) & 0xff;
  buf[16] = (declared >> 8) & 0xff;
  buf[17] = declared & 0xff;
  buf.set(data, ARTDMX_HEADER_LENGTH);

  return buf;
}

/**
 * Art-Net sequence numbers wrap 1..255 with 0 meaning "not implemented".
 * Returns true when `next` is genuinely newer than `prev`, so out-of-order
 * datagrams can be dropped instead of making the rig stutter.
 */
export function isNewerSequence(prev: number, next: number): boolean {
  if (next === 0 || prev === 0) return true;
  const diff = (next - prev + 256) % 256;
  return diff !== 0 && diff < 128;
}
