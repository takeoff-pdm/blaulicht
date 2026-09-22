/** Shape of the bridge that `preload` exposes to the renderer. */

import type { DemoPattern } from './console.js';
import type { StoreStats } from './dmxstore.js';
import type { Show } from './showfile.js';

/** A `Show` as it survives structured cloning: profiles carry functions, so the
 *  renderer re-attaches them from its own profile table after receiving it. */
export interface SerializedShow {
  name: string;
  path: string | null;
  formatVersion: number | null;
  json: unknown;
}

export interface ReceiverStatus {
  running: boolean;
  address: string | null;
  port: number;
  error: string | null;
}

export interface DemoStatus {
  enabled: boolean;
  pattern: DemoPattern;
  target: string;
}

export interface AppStatus {
  receiver: ReceiverStatus;
  demo: DemoStatus;
  stats: StoreStats;
}

export interface VisualizerApi {
  /** Resolves with the showfile the app was started with, if any. */
  getShowfile(): Promise<SerializedShow | null>;
  /** Opens a file dialog and loads the chosen showfile. */
  openShowfile(): Promise<SerializedShow | null>;
  loadShowfile(path: string): Promise<SerializedShow>;
  getStatus(): Promise<AppStatus>;
  setReceiverPort(port: number): Promise<ReceiverStatus>;
  setDemo(enabled: boolean, pattern: DemoPattern): Promise<DemoStatus>;

  onFrame(handler: (frame: ArrayBuffer) => void): () => void;
  onStatus(handler: (status: AppStatus) => void): () => void;
  onShowfile(handler: (show: SerializedShow) => void): () => void;
}

export type { Show, DemoPattern, StoreStats };
