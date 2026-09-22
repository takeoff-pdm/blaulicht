/**
 * Renderer entry point: owns the showfile -> layout -> stage pipeline, decodes
 * incoming DMX frames every animation frame, and drives the HUD.
 */

import { DEMO_PATTERNS, type DemoPattern } from '../core/console.js';
import { decodeFrame } from '../core/frame.js';
import { buildLayout, type Layout } from '../core/layout.js';
import { parseShowfile, type Show } from '../core/showfile.js';
import { DMX_UNIVERSE_SIZE } from '../core/artnet.js';
import type { AppStatus, SerializedShow, VisualizerApi } from '../core/ipc.js';
import type { FixtureState } from '../core/profiles.js';
import { Stage } from './stage.js';

declare global {
  interface Window {
    visualizer: VisualizerApi;
  }
}

const api = window.visualizer;
const $ = <T extends HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

const canvas = $<HTMLCanvasElement>('stage');
const stage = new Stage(canvas);

/** Live channel data, keyed by universe. Universes the rig patches but never
 *  receives stay zeroed rather than missing. */
const universes = new Map<number, Uint8Array>();

let show: Show | null = null;
let layout: Layout | null = null;
let states: FixtureState[] = [];
let selectedIndex = -1;
let latestStatus: AppStatus | null = null;

//
// Showfile handling.
//

function applyShowfile(serialized: SerializedShow): void {
  // The main process ships raw JSON; profiles carry functions and cannot cross
  // the structured-clone boundary, so the renderer re-parses it.
  show = parseShowfile(serialized.json, serialized.name);
  layout = buildLayout(show);
  states = layout.fixtures.map(() => ({
    r: 0, g: 0, b: 0, alpha: 0, pan: 128, tilt: 128, strobe: 0, focus: 0,
  }));

  universes.clear();
  for (const universe of show.universes) {
    universes.set(universe, new Uint8Array(DMX_UNIVERSE_SIZE));
  }

  stage.setShow(show, layout);
  stage.applySettings();

  $('empty').classList.add('hidden');
  $('topbar').classList.remove('hidden');
  $('sidebar').classList.remove('hidden');

  $('stat-show').textContent = serialized.name;
  $('stat-fixtures').textContent = String(show.fixtures.length);
  $('stat-universes').textContent = String(show.universes.length);

  renderWarnings(show);
  renderUniverseList();
  closeInspector();
}

function renderWarnings(show: Show): void {
  const section = $('warnings-section');
  const list = $('warnings');
  list.innerHTML = '';
  if (show.warnings.length === 0) {
    section.classList.add('hidden');
    return;
  }
  section.classList.remove('hidden');
  // Repeated unknown-profile warnings collapse into one line with a count.
  const counts = new Map<string, number>();
  for (const w of show.warnings) counts.set(w, (counts.get(w) ?? 0) + 1);
  for (const [text, count] of counts) {
    const li = document.createElement('li');
    li.textContent = count > 1 ? `${text} (x${count})` : text;
    list.appendChild(li);
  }
}

function renderUniverseList(): void {
  const container = $('universe-list');
  container.innerHTML = '';
  if (!show) return;
  for (const universe of show.universes) {
    const row = document.createElement('div');
    row.className = 'universe';
    row.dataset.universe = String(universe);
    row.innerHTML =
      `<span>U${universe}</span><span class="bar"><i style="width:0%"></i></span>` +
      `<span class="rate">0/s</span>`;
    container.appendChild(row);
  }
}

//
// DMX intake.
//

api.onFrame((buffer) => {
  for (const entry of decodeFrame(buffer)) {
    let target = universes.get(entry.universe);
    if (!target) {
      target = new Uint8Array(DMX_UNIVERSE_SIZE);
      universes.set(entry.universe, target);
    }
    target.set(entry.data);
  }
});

const EMPTY_UNIVERSE = new Uint8Array(DMX_UNIVERSE_SIZE);

function decodeStates(): void {
  if (!layout) return;
  for (let i = 0; i < layout.fixtures.length; i++) {
    const fixture = layout.fixtures[i];
    const data = universes.get(fixture.universe) ?? EMPTY_UNIVERSE;
    states[i] = fixture.profile.decode(data, fixture.base);
  }
}

//
// Render loop.
//

let frames = 0;
let fpsWindowStart = performance.now();
const startedAt = performance.now();

function tick(): void {
  requestAnimationFrame(tick);

  const now = performance.now();
  if (layout) {
    decodeStates();
    stage.update(states, (now - startedAt) / 1000);
    if (selectedIndex >= 0) updateInspectorValues();
  }
  stage.render();

  frames++;
  if (now - fpsWindowStart >= 500) {
    $('stat-fps').textContent = Math.round((frames * 1000) / (now - fpsWindowStart)).toString();
    frames = 0;
    fpsWindowStart = now;
  }
}

//
// Status HUD.
//

function applyStatus(status: AppStatus): void {
  latestStatus = status;

  const { receiver, demo, stats } = status;
  $('stat-artnet').textContent = receiver.running
    ? (receiver.address ?? `:${receiver.port}`)
    : 'offline';

  const totalRate = stats.universes.reduce((sum, u) => sum + u.rate, 0);
  $('stat-rate').textContent = String(totalRate);

  const errorBox = $('receiver-error');
  if (receiver.error) {
    errorBox.textContent = receiver.error;
    errorBox.classList.remove('hidden');
  } else {
    errorBox.classList.add('hidden');
  }

  ($('demo-enabled') as HTMLInputElement).checked = demo.enabled;
  // The main process owns the active pattern (it can be set from the command
  // line), so the control follows it rather than the other way round.
  const patternControl = $('demo-pattern') as HTMLSelectElement;
  if (patternControl.value !== demo.pattern) patternControl.value = demo.pattern;

  const byUniverse = new Map(stats.universes.map((u) => [u.universe, u]));
  for (const row of document.querySelectorAll<HTMLElement>('.universe')) {
    const universe = Number(row.dataset.universe);
    const info = byUniverse.get(universe);
    const bar = row.querySelector('i') as HTMLElement;
    const rate = row.querySelector('.rate') as HTMLElement;
    // 44 Hz is a full-rate DMX stream; scale the bar against that.
    const fraction = info ? Math.min(1, info.rate / 44) : 0;
    bar.style.width = `${Math.round(fraction * 100)}%`;
    rate.textContent = `${info?.rate ?? 0}/s`;
    row.classList.toggle('stale', !info || info.rate === 0);
  }
}

api.onStatus(applyStatus);
api.onShowfile(applyShowfile);

//
// Inspector.
//

function closeInspector(): void {
  selectedIndex = -1;
  $('inspector').classList.add('hidden');
}

function openInspector(index: number): void {
  if (!layout) return;
  selectedIndex = index;
  const fixture = layout.fixtures[index];
  $('inspector').classList.remove('hidden');
  $('insp-name').textContent = fixture.name;
  $('insp-group').textContent = `${fixture.groupName} (${fixture.groupKey})`;
  $('insp-profile').textContent = `${fixture.kind} · ${fixture.model}`;
  $('insp-patch').textContent =
    `U${fixture.universe} @ ${fixture.startAddr}–${fixture.startAddr + fixture.profile.footprint - 1}`;

  const channels = $('insp-channels');
  channels.innerHTML = '';
  fixture.profile.channels.forEach((name, i) => {
    const label = document.createElement('span');
    label.className = 'ch-name';
    label.textContent = `${fixture.startAddr + i} ${name}`;
    const value = document.createElement('span');
    value.dataset.channel = String(i);
    value.textContent = '0';
    channels.append(label, value);
  });
}

function updateInspectorValues(): void {
  if (!layout || selectedIndex < 0) return;
  const fixture = layout.fixtures[selectedIndex];
  const state = states[selectedIndex];
  const data = universes.get(fixture.universe) ?? EMPTY_UNIVERSE;

  $('insp-alpha').textContent = `${state.alpha} (${Math.round((state.alpha / 255) * 100)}%)`;
  $('insp-colour').textContent = `${state.r}, ${state.g}, ${state.b}`;
  $('insp-pantilt').textContent = `${state.pan} / ${state.tilt}`;
  $('insp-strobe').textContent = state.strobe === 0 ? 'off' : String(state.strobe);

  for (const el of document.querySelectorAll<HTMLElement>('#insp-channels [data-channel]')) {
    const index = Number(el.dataset.channel);
    el.textContent = String(data[fixture.base + index] ?? 0);
  }
}

canvas.addEventListener('pointerdown', (event) => {
  if (event.button !== 0 || !layout) return;
  const hit = stage.pick(event.clientX, event.clientY);
  if (!hit) {
    closeInspector();
    return;
  }
  openInspector(layout.fixtures.indexOf(hit));
});

$('inspector-close').addEventListener('click', closeInspector);

//
// Controls.
//

async function openShowfile(): Promise<void> {
  try {
    const result = await api.openShowfile();
    if (result) applyShowfile(result);
  } catch (err) {
    alert(`Could not open showfile:\n\n${(err as Error).message}`);
  }
}

$('open-showfile').addEventListener('click', () => void openShowfile());
$('empty-open').addEventListener('click', () => void openShowfile());

const patternSelect = $<HTMLSelectElement>('demo-pattern');
for (const pattern of DEMO_PATTERNS) {
  const option = document.createElement('option');
  option.value = pattern;
  option.textContent = pattern;
  patternSelect.appendChild(option);
}

function syncDemo(): void {
  const enabled = ($('demo-enabled') as HTMLInputElement).checked;
  void api.setDemo(enabled, patternSelect.value as DemoPattern);
}

$('demo-enabled').addEventListener('change', syncDemo);
patternSelect.addEventListener('change', syncDemo);

$('port').addEventListener('change', (event) => {
  const port = Number((event.target as HTMLInputElement).value);
  void api.setReceiverPort(port).then((receiver) => {
    if (latestStatus) applyStatus({ ...latestStatus, receiver });
  });
});

function bindToggle(id: string, key: 'beams' | 'pools' | 'bloom' | 'room'): void {
  $(id).addEventListener('change', (event) => {
    stage.settings[key] = (event.target as HTMLInputElement).checked;
    stage.applySettings();
  });
}
bindToggle('opt-beams', 'beams');
bindToggle('opt-pools', 'pools');
bindToggle('opt-bloom', 'bloom');
bindToggle('opt-room', 'room');

function bindSlider(id: string, key: 'haze' | 'gain' | 'exposure'): void {
  $(id).addEventListener('input', (event) => {
    stage.settings[key] = Number((event.target as HTMLInputElement).value);
    stage.applySettings();
  });
}
bindSlider('opt-haze', 'haze');
bindSlider('opt-gain', 'gain');
bindSlider('opt-exposure', 'exposure');

window.addEventListener('resize', () => stage.resize());

//
// Boot.
//

async function boot(): Promise<void> {
  const [initial, status] = await Promise.all([api.getShowfile(), api.getStatus()]);
  if (initial) applyShowfile(initial);
  applyStatus(status);
  ($('port') as HTMLInputElement).value = String(status.receiver.port);
  tick();
}

void boot();
