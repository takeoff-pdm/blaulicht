/**
 * End-to-end: demo console -> DMX encode -> Art-Net -> real UDP socket ->
 * receiver -> store -> profile decode.
 *
 * This is the path every lit pixel in the app travels, so it is tested with a
 * live socket rather than by calling the parser directly.
 */

import { createSocket, type Socket } from 'node:dgram';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { buildArtDmxPacket } from '../src/core/artnet.js';
import { renderDemoFrame } from '../src/core/console.js';
import { buildLayout, type Layout } from '../src/core/layout.js';
import { parseShowfile } from '../src/core/showfile.js';
import { ArtNetReceiver } from '../src/main/receiver.js';
import { DemoSender } from '../src/main/sender.js';

const MINI = JSON.parse(
  readFileSync(resolve(import.meta.dirname, 'fixtures/mini-show.json'), 'utf8'),
);

function layoutOf(json: unknown): Layout {
  return buildLayout(parseShowfile(json));
}

/** Resolves once `predicate` holds, or rejects after `timeoutMs`. */
async function waitFor(
  predicate: () => boolean,
  timeoutMs = 5000,
  label = 'condition',
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error(`Timed out waiting for ${label}.`);
    await new Promise((r) => setTimeout(r, 10));
  }
}

describe('Art-Net pipeline', () => {
  let receiver: ArtNetReceiver;
  let port: number;
  let client: Socket;

  beforeEach(async () => {
    // Port 0 lets the OS pick a free port, so tests never collide with a real
    // Art-Net node on 6454.
    receiver = new ArtNetReceiver({ port: 0, bindAddress: '127.0.0.1' });
    port = (await receiver.start()).port;
    client = createSocket('udp4');
  });

  afterEach(async () => {
    client.close();
    await receiver.stop();
  });

  const send = (packet: Uint8Array): Promise<void> =>
    new Promise((done, fail) =>
      client.send(packet, port, '127.0.0.1', (err) => (err ? fail(err) : done())),
    );

  it('delivers channel data from the wire into the store', async () => {
    const data = new Uint8Array(512);
    data[0] = 11;
    data[1] = 22;
    data[511] = 33;

    await send(buildArtDmxPacket({ universe: 1, data, sequence: 1 }));
    await waitFor(() => receiver.store.has(1), 5000, 'universe 1');

    const stored = receiver.store.get(1)!;
    expect(stored[0]).toBe(11);
    expect(stored[1]).toBe(22);
    expect(stored[511]).toBe(33);
  });

  it('reproduces the demo console exactly, fixture by fixture', async () => {
    const layout = layoutOf(MINI);
    const time = 3.25; // the console is deterministic in time
    const frame = renderDemoFrame(layout, 'rainbow-chase', time);
    expect(frame.size).toBeGreaterThan(0);

    let sequence = 1;
    for (const [universe, data] of frame) {
      await send(buildArtDmxPacket({ universe, data, sequence: sequence++ }));
    }

    await waitFor(
      () => [...frame.keys()].every((u) => receiver.store.get(u) !== undefined),
      5000,
      'every demo universe',
    );

    // Every fixture must decode to the same state on both sides of the wire.
    let litFixtures = 0;
    for (const fixture of layout.fixtures) {
      const sent = frame.get(fixture.universe)!;
      const received = receiver.store.get(fixture.universe)!;

      const expected = fixture.profile.decode(sent, fixture.base);
      const actual = fixture.profile.decode(received, fixture.base);
      expect(actual, `${fixture.name} (${fixture.model})`).toEqual(expected);

      if (actual.alpha > 0 && actual.r + actual.g + actual.b > 0) litFixtures++;
    }

    // A pattern that lights nothing would pass the equality check vacuously.
    expect(litFixtures).toBeGreaterThan(0);
  });

  it('counts a malformed datagram as a rejection instead of crashing', async () => {
    const rejections: string[] = [];
    receiver.on('rejected', (reason) => rejections.push(reason));

    await send(Uint8Array.from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]));
    await waitFor(() => rejections.length > 0, 5000, 'a rejection');

    expect(rejections[0]).toBe('bad-id');
    expect(receiver.store.stats(0).rejected['bad-id']).toBe(1);
    expect(receiver.running).toBe(true);
  });

  it('ignores a replayed packet so the rig does not stutter', async () => {
    const first = new Uint8Array(512);
    first[0] = 200;
    const replay = new Uint8Array(512);
    replay[0] = 5;

    await send(buildArtDmxPacket({ universe: 0, data: first, sequence: 20 }));
    await waitFor(() => receiver.store.get(0)?.[0] === 200, 5000, 'the first packet');

    await send(buildArtDmxPacket({ universe: 0, data: replay, sequence: 19 }));
    await waitFor(() => receiver.store.stats(0).rejected.stale === 1, 5000, 'the replay to be dropped');

    expect(receiver.store.get(0)![0]).toBe(200);
  });

  it('keeps universes independent', async () => {
    const a = new Uint8Array(512);
    a[0] = 1;
    const b = new Uint8Array(512);
    b[0] = 2;

    await send(buildArtDmxPacket({ universe: 0, data: a, sequence: 1 }));
    await send(buildArtDmxPacket({ universe: 7, data: b, sequence: 1 }));
    await waitFor(() => receiver.store.has(0) && receiver.store.has(7), 5000, 'both universes');

    expect(receiver.store.get(0)![0]).toBe(1);
    expect(receiver.store.get(7)![0]).toBe(2);
  });

  it('streams continuously from the shipped demo sender', async () => {
    const sender = new DemoSender({ port, fps: 44 });
    sender.setLayout(layoutOf(MINI));
    sender.setPattern('beam-fan');
    sender.start();

    try {
      // Enough frames to prove it is a stream, not a single burst.
      await waitFor(
        () => receiver.store.stats(Date.now()).totalPackets >= 20,
        8000,
        'a sustained demo stream',
      );
    } finally {
      sender.stop();
    }

    const stats = receiver.store.stats(Date.now());
    expect(stats.totalPackets).toBeGreaterThanOrEqual(20);
    expect(stats.rejected.stale).toBe(0);
    // Both of the showfile's universes must be carried.
    expect(stats.universes.map((u) => u.universe).sort()).toEqual([0, 1]);
  });

  it('refuses to start twice on the same socket', async () => {
    await expect(receiver.start()).rejects.toThrow(/already running/);
  });
});

describe('demo console determinism', () => {
  it('produces identical DMX for identical time', () => {
    const layout = layoutOf(MINI);
    const a = renderDemoFrame(layout, 'colour-wash', 7.5);
    const b = renderDemoFrame(layout, 'colour-wash', 7.5);
    for (const [universe, data] of a) {
      expect([...b.get(universe)!]).toEqual([...data]);
    }
  });

  it('actually animates between frames', () => {
    const layout = layoutOf(MINI);
    const a = renderDemoFrame(layout, 'rainbow-chase', 0);
    const b = renderDemoFrame(layout, 'rainbow-chase', 1.7);
    const changed = [...a.keys()].some((u) =>
      [...a.get(u)!].some((v, i) => v !== b.get(u)![i]),
    );
    expect(changed).toBe(true);
  });

  it('blacks every fixture out on the blackout pattern', () => {
    const layout = layoutOf(MINI);
    const frame = renderDemoFrame(layout, 'blackout', 2);
    for (const fixture of layout.fixtures) {
      const state = fixture.profile.decode(frame.get(fixture.universe)!, fixture.base);
      const lit = state.alpha > 0 && state.r + state.g + state.b > 0;
      expect(lit, `${fixture.name} should be dark`).toBe(false);
    }
  });

  it('only writes to the universes the showfile patches', () => {
    const layout = layoutOf(MINI);
    const frame = renderDemoFrame(layout, 'group-sweep', 1);
    expect([...frame.keys()].sort()).toEqual([0, 1]);
  });
});
