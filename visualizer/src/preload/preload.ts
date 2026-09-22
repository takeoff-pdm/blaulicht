/**
 * Context-isolated bridge. The renderer gets a narrow, typed surface and no
 * access to Node, Electron or the network.
 */

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron';

import type { AppStatus, SerializedShow, VisualizerApi } from '../core/ipc.js';
import type { DemoPattern } from '../core/console.js';

function subscribe<T>(channel: string, handler: (payload: T) => void): () => void {
  const listener = (_event: IpcRendererEvent, payload: T) => handler(payload);
  ipcRenderer.on(channel, listener);
  return () => ipcRenderer.removeListener(channel, listener);
}

const api: VisualizerApi = {
  getShowfile: () => ipcRenderer.invoke('showfile:get'),
  openShowfile: () => ipcRenderer.invoke('showfile:open'),
  loadShowfile: (path: string) => ipcRenderer.invoke('showfile:load', path),
  getStatus: () => ipcRenderer.invoke('app:status'),
  setReceiverPort: (port: number) => ipcRenderer.invoke('receiver:port', port),
  setDemo: (enabled: boolean, pattern: DemoPattern) =>
    ipcRenderer.invoke('demo:set', enabled, pattern),

  onFrame: (handler) => subscribe<ArrayBuffer>('dmx:frame', handler),
  onStatus: (handler) => subscribe<AppStatus>('app:status', handler),
  onShowfile: (handler) => subscribe<SerializedShow>('showfile:changed', handler),
};

contextBridge.exposeInMainWorld('visualizer', api);
