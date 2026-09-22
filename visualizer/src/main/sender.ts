/**
 * Demo Art-Net sender.
 *
 * Rather than injecting generated DMX straight into the store, the demo console
 * transmits real Art-Net datagrams at the receiver. Everything the renderer
 * shows in demo mode therefore travelled the same code path as traffic from a
 * real blaulicht instance — which is exactly what makes the demo useful as a
 * smoke test.
 */

import { createSocket, type Socket } from 'node:dgram';

import { buildArtDmxPacket } from '../core/artnet.js';
import { renderDemoFrame } from '../core/console.js';
import type { DemoPattern } from '../core/console.js';
import type { Layout } from '../core/layout.js';

export interface DemoSenderOptions {
  host?: string;
  port: number;
  /** Frames per second. 44 matches the DMX512 refresh rate. */
  fps?: number;
}

export class DemoSender {
  private socket: Socket | null = null;
  private timer: NodeJS.Timeout | null = null;
  private sequence = 1;
  private startedAt = 0;
  private layout: Layout | null = null;
  private pattern: DemoPattern = 'rainbow-chase';

  constructor(private readonly options: DemoSenderOptions) {}

  setLayout(layout: Layout | null): void {
    this.layout = layout;
  }

  setPattern(pattern: DemoPattern): void {
    this.pattern = pattern;
  }

  get running(): boolean {
    return this.timer !== null;
  }

  start(): void {
    if (this.timer) return;
    this.socket = createSocket('udp4');
    this.startedAt = Date.now();
    const interval = 1000 / (this.options.fps ?? 44);
    this.timer = setInterval(() => this.tick(), interval);
  }

  private tick(): void {
    const { layout, socket } = this;
    if (!layout || !socket) return;

    const time = (Date.now() - this.startedAt) / 1000;
    const universes = renderDemoFrame(layout, this.pattern, time);
    const host = this.options.host ?? '127.0.0.1';

    for (const [universe, data] of universes) {
      const packet = buildArtDmxPacket({
        universe,
        data,
        sequence: this.sequence,
      });
      socket.send(packet, this.options.port, host, () => {
        // Errors here are non-fatal: the receiver simply sees no traffic.
      });
    }

    // Art-Net sequence wraps 1..255; 0 means "sequencing disabled".
    this.sequence = this.sequence >= 255 ? 1 : this.sequence + 1;
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.socket?.close();
    this.socket = null;
  }
}
