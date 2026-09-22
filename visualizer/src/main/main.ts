/**
 * Electron main process.
 *
 * Owns the Art-Net socket, the showfile on disk and the demo console, and
 * pushes packed DMX frames to the renderer. The renderer never touches the
 * network or the filesystem.
 */

import { app, BrowserWindow, dialog, ipcMain, Menu, type IpcMainInvokeEvent } from 'electron';
import { readFile, writeFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { ARTNET_PORT } from '../core/artnet.js';
import { DEMO_PATTERNS, type DemoPattern } from '../core/console.js';
import { encodeFrame, type FrameEntry } from '../core/frame.js';
import { buildLayout, type Layout } from '../core/layout.js';
import { parseShowfile } from '../core/showfile.js';
import type { AppStatus, SerializedShow } from '../core/ipc.js';
import { ArtNetReceiver } from './receiver.js';
import { DemoSender } from './sender.js';

const DIRNAME = typeof __dirname !== 'undefined' ? __dirname : fileURLToPath(new URL('.', import.meta.url));

/** Renderer push rate. DMX itself runs at 44 Hz; 60 keeps interpolation smooth. */
const FRAME_HZ = 60;
const STATUS_HZ = 4;

interface AppState {
  window: BrowserWindow | null;
  receiver: ArtNetReceiver | null;
  sender: DemoSender | null;
  layout: Layout | null;
  showfile: SerializedShow | null;
  port: number;
  pattern: DemoPattern;
  receiverError: string | null;
  receiverAddress: string | null;
}

const state: AppState = {
  window: null,
  receiver: null,
  sender: null,
  layout: null,
  showfile: null,
  port: ARTNET_PORT,
  pattern: 'rainbow-chase',
  receiverError: null,
  receiverAddress: null,
};

function status(): AppStatus {
  const receiver = state.receiver;
  return {
    receiver: {
      running: receiver?.running ?? false,
      address: state.receiverAddress,
      port: state.port,
      error: state.receiverError,
    },
    demo: {
      enabled: state.sender?.running ?? false,
      pattern: state.pattern,
      target: `127.0.0.1:${state.port}`,
    },
    stats: receiver?.store.stats(Date.now()) ?? {
      universes: [],
      totalPackets: 0,
      rejected: {
        'too-short': 0,
        'bad-id': 0,
        'unsupported-opcode': 0,
        'bad-protocol-version': 0,
        'bad-length': 0,
        truncated: 0,
        stale: 0,
      },
    },
  };
}

async function loadShowfileFrom(path: string): Promise<SerializedShow> {
  const text = await readFile(path, 'utf8');
  const json = JSON.parse(text);
  // Parse here too so a broken showfile fails in the main process with a clear
  // message instead of blanking the renderer.
  const show = parseShowfile(json, basename(path));
  state.layout = buildLayout(show);
  state.sender?.setLayout(state.layout);

  const serialized: SerializedShow = {
    name: basename(path),
    path,
    formatVersion: show.formatVersion,
    json,
  };
  state.showfile = serialized;
  return serialized;
}

async function startReceiver(port: number): Promise<void> {
  await state.receiver?.stop();
  state.receiver = null;
  state.receiverAddress = null;
  state.receiverError = null;

  const receiver = new ArtNetReceiver({ port });
  receiver.on('error', (err) => {
    state.receiverError = err.message;
  });

  try {
    const address = await receiver.start();
    state.receiver = receiver;
    state.receiverAddress = `${address.address}:${address.port}`;
    state.port = address.port;
  } catch (err) {
    state.receiverError = (err as Error).message;
  }
}

function pushFrames(): void {
  const { receiver, window } = state;
  if (!receiver || !window || window.isDestroyed()) return;

  const dirty = receiver.store.dirty();
  if (dirty.length === 0) return;

  const entries: FrameEntry[] = dirty.map((universe) => ({
    universe,
    data: receiver.store.getOrCreate(universe),
  }));
  receiver.store.clearDirty();

  window.webContents.send('dmx:frame', encodeFrame(entries));
}

function pushStatus(): void {
  const { window } = state;
  if (!window || window.isDestroyed()) return;
  window.webContents.send('app:status', status());
}

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1600,
    height: 950,
    backgroundColor: '#05060a',
    title: 'blaulicht visualizer',
    show: false,
    webPreferences: {
      preload: join(DIRNAME, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      // The renderer is the only WebGL consumer; keep it off the main thread.
      backgroundThrottling: false,
    },
  });

  window.once('ready-to-show', () => window.show());

  const devServer = process.env.VITE_DEV_SERVER_URL;
  if (devServer) {
    void window.loadURL(devServer);
  } else {
    void window.loadFile(join(DIRNAME, '../renderer/index.html'));
  }

  return window;
}

function setDemo(enabled: boolean, pattern: DemoPattern) {
  if (!DEMO_PATTERNS.includes(pattern)) {
    throw new Error(`Unknown demo pattern "${pattern}".`);
  }
  state.pattern = pattern;

  if (!enabled) {
    state.sender?.stop();
    state.sender = null;
    return status().demo;
  }

  if (!state.sender) state.sender = new DemoSender({ port: state.port });
  state.sender.setLayout(state.layout);
  state.sender.setPattern(pattern);
  state.sender.start();
  return status().demo;
}

function registerIpc(): void {
  ipcMain.handle('showfile:get', () => state.showfile);

  ipcMain.handle('showfile:open', async () => {
    const result = await dialog.showOpenDialog({
      title: 'Open blaulicht showfile',
      filters: [{ name: 'blaulicht showfile', extensions: ['json'] }],
      properties: ['openFile'],
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    const show = await loadShowfileFrom(result.filePaths[0]);
    state.window?.webContents.send('showfile:changed', show);
    return show;
  });

  ipcMain.handle('showfile:load', async (_event: IpcMainInvokeEvent, path: string) => {
    const show = await loadShowfileFrom(resolve(path));
    state.window?.webContents.send('showfile:changed', show);
    return show;
  });

  ipcMain.handle('app:status', () => status());

  ipcMain.handle('receiver:port', async (_event: IpcMainInvokeEvent, port: number) => {
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      throw new Error(`Invalid port ${port}.`);
    }
    const wasSending = state.sender?.running ?? false;
    state.sender?.stop();
    await startReceiver(port);
    if (wasSending) {
      state.sender = new DemoSender({ port: state.port });
      state.sender.setLayout(state.layout);
      state.sender.setPattern(state.pattern);
      state.sender.start();
    }
    return status().receiver;
  });

  ipcMain.handle(
    'demo:set',
    (_event: IpcMainInvokeEvent, enabled: boolean, pattern: DemoPattern) =>
      setDemo(enabled, pattern),
  );
}

/** Showfile passed on the command line: `electron . path/to/show.json`. */
function showfileFromArgv(): string | null {
  const args = process.argv.slice(app.isPackaged ? 1 : 2);
  const candidate = args.find((arg) => arg.endsWith('.json') && !arg.startsWith('-'));
  return candidate ? resolve(candidate) : null;
}

/**
 * Automated smoke run (`VISUALIZER_SMOKE=1`).
 *
 * Boots the app for real, drives it with the demo console over a live Art-Net
 * socket, then reads the rendered framebuffer back and reports what it found.
 * This is what proves the renderer actually draws light, rather than merely
 * that the modules import.
 */
async function runSmoke(window: BrowserWindow): Promise<void> {
  const durationMs = Number(process.env.VISUALIZER_SMOKE_MS ?? 4000);
  const screenshotPath = process.env.VISUALIZER_SMOKE_PNG ?? null;

  const report = (result: Record<string, unknown>) => {
    // A marker line the test harness greps for.
    process.stdout.write(`\nSMOKE_RESULT ${JSON.stringify(result)}\n`);
  };

  try {
    await new Promise<void>((done) => {
      if (!window.webContents.isLoading()) return done();
      window.webContents.once('did-finish-load', () => done());
    });

    setDemo(true, (process.env.VISUALIZER_SMOKE_PATTERN as DemoPattern) ?? 'rainbow-chase');
    await new Promise((r) => setTimeout(r, durationMs));

    const image = await window.webContents.capturePage();
    const { width, height } = image.getSize();
    const bitmap = image.toBitmap(); // BGRA

    // How much of the frame is actually lit, and how bright the brightest
    // pixel got. A black window would fail both.
    let lit = 0;
    let peak = 0;
    let sum = 0;
    const pixels = bitmap.length / 4;
    for (let i = 0; i < bitmap.length; i += 4) {
      const luminance = 0.0722 * bitmap[i] + 0.7152 * bitmap[i + 1] + 0.2126 * bitmap[i + 2];
      sum += luminance;
      if (luminance > peak) peak = luminance;
      if (luminance > 24) lit++;
    }

    const rendererState = await window.webContents.executeJavaScript(`
      ({
        fixtures: Number(document.getElementById('stat-fixtures').textContent),
        universes: Number(document.getElementById('stat-universes').textContent),
        fps: Number(document.getElementById('stat-fps').textContent),
        packetsPerSecond: Number(document.getElementById('stat-rate').textContent),
        artnet: document.getElementById('stat-artnet').textContent,
        show: document.getElementById('stat-show').textContent,
      })
    `);

    if (screenshotPath) {
      await writeFile(screenshotPath, image.toPNG());
    }

    report({
      ok: true,
      width,
      height,
      litFraction: pixels > 0 ? lit / pixels : 0,
      peakLuminance: peak,
      meanLuminance: pixels > 0 ? sum / pixels : 0,
      screenshotPath,
      renderer: rendererState,
      stats: status().stats,
    });
    app.exit(0);
  } catch (err) {
    report({ ok: false, error: (err as Error).message });
    app.exit(1);
  }
}

async function main(): Promise<void> {
  await app.whenReady();

  registerIpc();
  Menu.setApplicationMenu(null);
  state.window = createWindow();

  await startReceiver(state.port);

  const initial = showfileFromArgv() ?? process.env.BLAULICHT_SHOWFILE ?? null;
  if (initial) {
    try {
      await loadShowfileFrom(initial);
    } catch (err) {
      dialog.showErrorBox('Could not open showfile', `${initial}\n\n${(err as Error).message}`);
    }
  }

  const frameTimer = setInterval(pushFrames, 1000 / FRAME_HZ);
  const statusTimer = setInterval(pushStatus, 1000 / STATUS_HZ);

  app.on('window-all-closed', () => {
    clearInterval(frameTimer);
    clearInterval(statusTimer);
    state.sender?.stop();
    void state.receiver?.stop();
    app.quit();
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) state.window = createWindow();
  });

  if (process.env.VISUALIZER_SMOKE === '1' && state.window) {
    await runSmoke(state.window);
  }
}

void main();
