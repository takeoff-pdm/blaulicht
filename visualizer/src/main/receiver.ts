/**
 * Art-Net UDP receiver.
 *
 * Owns the socket and a `DmxStore`; everything protocol-shaped lives in
 * `src/core` so this file stays a thin Node binding and the test-suite can
 * drive a real socket end to end.
 */

import { createSocket, type Socket } from 'node:dgram';
import { EventEmitter } from 'node:events';

import { ARTNET_PORT, ArtNetParseError, parseArtNetPacket } from '../core/artnet.js';
import { DmxStore } from '../core/dmxstore.js';

export interface ReceiverOptions {
  port?: number;
  /** Bind address. `0.0.0.0` receives unicast, broadcast and (joined) multicast. */
  bindAddress?: string;
  /** Universes to preallocate so the HUD lists them before any traffic. */
  universes?: number[];
  /** Injected in tests to make rate maths deterministic. */
  now?: () => number;
}

export interface ReceiverAddress {
  address: string;
  port: number;
}

type Events = {
  listening: [ReceiverAddress];
  /** A universe's channel data changed. */
  dmx: [number];
  /** A datagram was rejected; carries the reason and the sender. */
  rejected: [string, string];
  error: [Error];
};

export class ArtNetReceiver extends EventEmitter<Events> {
  readonly store: DmxStore;
  private socket: Socket | null = null;
  private readonly port: number;
  private readonly bindAddress: string;
  private readonly now: () => number;

  constructor(options: ReceiverOptions = {}) {
    super();
    this.port = options.port ?? ARTNET_PORT;
    this.bindAddress = options.bindAddress ?? '0.0.0.0';
    this.now = options.now ?? (() => Date.now());
    this.store = new DmxStore(options.universes ?? []);
  }

  start(): Promise<ReceiverAddress> {
    if (this.socket) {
      return Promise.reject(new Error('Receiver is already running.'));
    }

    return new Promise((resolve, reject) => {
      // `reuseAddr` lets the visualiser run alongside another Art-Net node on
      // the same host, which is the normal case while designing a show.
      const socket = createSocket({ type: 'udp4', reuseAddr: true });
      this.socket = socket;

      const onStartupError = (err: Error) => {
        this.socket = null;
        socket.removeListener('error', onStartupError);
        socket.close();
        reject(err);
      };

      socket.once('error', onStartupError);
      socket.on('message', (msg, rinfo) => this.handle(msg, `${rinfo.address}:${rinfo.port}`));

      socket.bind(this.port, this.bindAddress, () => {
        socket.removeListener('error', onStartupError);
        socket.on('error', (err) => this.emit('error', err));
        try {
          socket.setBroadcast(true);
        } catch {
          // Broadcast is a nice-to-have; unicast reception still works without it.
        }
        const address = socket.address();
        const resolved = { address: address.address, port: address.port };
        this.emit('listening', resolved);
        resolve(resolved);
      });
    });
  }

  private handle(msg: Buffer, sender: string): void {
    let packet;
    try {
      packet = parseArtNetPacket(msg);
    } catch (err) {
      if (err instanceof ArtNetParseError) {
        this.store.recordRejection(err.reason);
        this.emit('rejected', err.reason, sender);
        return;
      }
      this.emit('error', err as Error);
      return;
    }

    if (packet === null || packet.type !== 'dmx') return;

    if (this.store.apply(packet, this.now())) {
      this.emit('dmx', packet.universe);
    }
  }

  stop(): Promise<void> {
    const socket = this.socket;
    this.socket = null;
    if (!socket) return Promise.resolve();
    return new Promise((resolve) => socket.close(() => resolve()));
  }

  get running(): boolean {
    return this.socket !== null;
  }
}
