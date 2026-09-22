/**
 * Full-application smoke test.
 *
 * Boots the packaged main + renderer bundles under a real Electron, drives the
 * rig with the demo console over a live Art-Net socket, then reads the
 * framebuffer back. It is the only test that can prove the renderer actually
 * puts light on screen.
 *
 * Skipped automatically when no Electron binary can run here (no display, or a
 * distro where the prebuilt binary cannot resolve its shared libraries).
 */

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const APP_DIR = resolve(import.meta.dirname, '..');
const REPO = resolve(APP_DIR, '..');
const SHOWFILE = resolve(REPO, 'SPARTACUS_DRAFT.json');

interface Launcher {
  command: string;
  args: string[];
  env: NodeJS.ProcessEnv;
}

/**
 * Finds a runnable Electron.
 *
 * The npm binary is preferred, but on NixOS it cannot load its shared
 * libraries, so a nixpkgs-provided Electron is used instead. The direnv shell
 * in this repo exports an `LD_LIBRARY_PATH` that makes that build fail too, so
 * it is stripped for the child.
 */
function findElectron(): Launcher | null {
  const clean = { ...process.env };
  delete clean.LD_LIBRARY_PATH;

  const local = join(APP_DIR, 'node_modules/electron/dist/electron');
  if (existsSync(local)) {
    const probe = spawnSync(local, ['--version'], { encoding: 'utf8', timeout: 30_000 });
    if (probe.status === 0) {
      return { command: local, args: [], env: process.env };
    }
  }

  const nix = spawnSync(
    'nix',
    ['--extra-experimental-features', 'nix-command flakes', 'shell', 'nixpkgs#electron',
     '--command', 'electron', '--version'],
    { encoding: 'utf8', timeout: 600_000, env: clean },
  );
  if (nix.status === 0) {
    return {
      command: 'nix',
      args: ['--extra-experimental-features', 'nix-command flakes', 'shell', 'nixpkgs#electron',
             '--command', 'electron'],
      env: clean,
    };
  }

  return null;
}

const electron = process.env.DISPLAY || process.env.WAYLAND_DISPLAY ? findElectron() : null;
const built = existsSync(join(APP_DIR, 'dist/main/main.cjs'))
  && existsSync(join(APP_DIR, 'dist/renderer/index.html'));

interface SmokeResult {
  ok: boolean;
  width: number;
  height: number;
  litFraction: number;
  peakLuminance: number;
  meanLuminance: number;
  renderer: {
    fixtures: number;
    universes: number;
    fps: number;
    packetsPerSecond: number;
    artnet: string;
    show: string;
  };
  stats: {
    totalPackets: number;
    universes: Array<{ universe: number; packets: number; outOfOrder: number; rate: number }>;
    rejected: Record<string, number>;
  };
}

function runSmoke(pattern: string): Promise<SmokeResult> {
  const screenshot = join(mkdtempSync(join(tmpdir(), 'blaulicht-smoke-')), `${pattern}.png`);

  return new Promise((done, fail) => {
    const child = spawn(electron!.command, [...electron!.args, APP_DIR], {
      cwd: APP_DIR,
      env: {
        ...electron!.env,
        VISUALIZER_SMOKE: '1',
        VISUALIZER_SMOKE_MS: '2500',
        VISUALIZER_SMOKE_PATTERN: pattern,
        VISUALIZER_SMOKE_PNG: screenshot,
        BLAULICHT_SHOWFILE: SHOWFILE,
      },
    });

    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));

    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      fail(new Error(`Electron did not finish in time.\n${stderr}`));
    }, 150_000);

    child.on('error', (err) => {
      clearTimeout(timer);
      fail(err);
    });

    child.on('close', (code) => {
      clearTimeout(timer);
      const marker = stdout.split('\n').find((line) => line.startsWith('SMOKE_RESULT '));
      if (!marker) {
        fail(new Error(`No smoke result (exit ${code}).\nstdout:\n${stdout}\nstderr:\n${stderr}`));
        return;
      }
      const result = JSON.parse(marker.slice('SMOKE_RESULT '.length)) as SmokeResult;
      expect(existsSync(screenshot), 'screenshot was written').toBe(true);
      done(result);
    });
  });
}

const canRun = Boolean(electron) && built && existsSync(SHOWFILE);

describe.skipIf(!canRun)('the application', () => {
  it('boots, receives its own Art-Net and renders a lit stage', { timeout: 180_000 }, async () => {
    const result = await runSmoke('rainbow-chase');

    expect(result.ok).toBe(true);
    expect(result.width).toBeGreaterThan(300);
    expect(result.height).toBeGreaterThan(300);

    // The showfile reached the renderer intact.
    expect(result.renderer.show).toBe('SPARTACUS_DRAFT.json');
    expect(result.renderer.fixtures).toBe(154);
    expect(result.renderer.universes).toBe(2);

    // Art-Net actually flowed, on both universes, with nothing rejected.
    expect(result.renderer.packetsPerSecond).toBeGreaterThan(20);
    expect(result.stats.totalPackets).toBeGreaterThan(50);
    expect(result.stats.universes.map((u) => u.universe)).toEqual([0, 1]);
    for (const universe of result.stats.universes) {
      expect(universe.packets).toBeGreaterThan(20);
      expect(universe.outOfOrder).toBe(0);
    }
    for (const [reason, count] of Object.entries(result.stats.rejected)) {
      expect(count, `rejected: ${reason}`).toBe(0);
    }

    // The window is genuinely lit, not a black canvas or a white blowout.
    expect(result.peakLuminance).toBeGreaterThan(120);
    expect(result.litFraction).toBeGreaterThan(0.02);
    expect(result.litFraction, 'the frame should not be blown out').toBeLessThan(0.85);
    expect(result.meanLuminance).toBeGreaterThan(2);

    // And it is rendering at an interactive rate with 154 fixtures.
    expect(result.renderer.fps).toBeGreaterThan(25);
  });

  it('renders a dark stage when the console blacks out', { timeout: 180_000 }, async () => {
    const result = await runSmoke('blackout');

    expect(result.ok).toBe(true);
    // Traffic still flows — the rig is simply dark. This separates "renders
    // what it is sent" from "renders something regardless".
    expect(result.stats.totalPackets).toBeGreaterThan(50);
    expect(result.litFraction).toBeLessThan(0.08);
    expect(result.meanLuminance).toBeLessThan(12);
  });
});

it('has been built before the smoke test runs', () => {
  // Fails loudly rather than silently skipping when `npm run build` was missed.
  expect(built, 'run `npm run build` first').toBe(true);
});
