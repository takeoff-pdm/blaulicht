// Pro DJ Link for the show server, as local HTTP: what is on each deck, the playlists on each
// player's media, track metadata and the audio files. No beat timing: built to work over WiFi.
//
//   node bridge.mjs            env: BRIDGE_PORT (8771), PROLINK_IFACE (name or IPv4, else the
//                              interface the first device is seen on), VCDJ_ID (7)
//
// Joins the network as a passive virtual CDJ (needed to get deck status, export.pdb and files).
// Everything comes from rekordbox-analysed media through NFS and the stick's export.pdb, which
// works with any player number, so the six real player slots stay free. NFS transfers (database
// and audio) run one at a time.
import http from 'node:http';
import { networkInterfaces } from 'node:os';
import { bringOnline, fetchFile, MediaSlot, DeviceType, CDJStatus } from 'prolink-connect';
const { PlayState } = CDJStatus;

const PORT = Number(process.env.BRIDGE_PORT ?? 8771);
const VCDJ_ID = Number(process.env.VCDJ_ID ?? 7);
const STALE_MS = 5000; // players send status about 5x a second; over WiFi packets go missing

const log = (...a) => console.log(new Date().toISOString(), ...a);

// ---- one NFS transfer at a time
let chain = Promise.resolve();
function serial(fn) {
  const run = chain.then(fn, fn);
  chain = run.catch(() => {});
  return run;
}
async function retry(fn, what, tries = 4) {
  for (let i = 1; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i >= tries) throw e;
      log(`${what} failed (${e.message}), retry ${i}`);
      await new Promise((r) => setTimeout(r, 500 * 2 ** i));
    }
  }
}

// ---- network
let network = null;
let netState = 'starting';
let netError = null;
const decks = new Map(); // deviceId -> deck row
const listeners = new Set(); // SSE responses

function ifaceInfo(nameOrIp) {
  for (const [name, list] of Object.entries(networkInterfaces())) {
    for (const i of list ?? []) if (i.family === 'IPv4' && (name === nameOrIp || i.address === nameOrIp)) return i;
  }
  throw new Error(`no IPv4 interface ${nameOrIp}`);
}

async function start() {
  network = await bringOnline();
  const dm = network.deviceManager;
  dm.on('connected', (d) => { log('device', d.id, d.name, d.ip.address); broadcast('devices', devices()); });
  dm.on('disconnected', (d) => {
    log('device lost', d.id, d.name);
    decks.delete(d.id);
    for (const k of [...playlistIndex.keys()]) if (k.startsWith(`${d.id}:`)) playlistIndex.delete(k);
    broadcast('devices', devices());
    broadcast('decks', deckRows());
  });
  netState = 'waiting for a device';
  if (process.env.PROLINK_IFACE) {
    network.configure({ iface: ifaceInfo(process.env.PROLINK_IFACE), vcdjId: VCDJ_ID, vcdjName: 'showserver' });
  } else {
    log('waiting for a Pro DJ Link device to pick the interface');
    await network.autoconfigFromPeers();
    network.configure({ ...currentConfig(), vcdjId: VCDJ_ID, vcdjName: 'showserver' });
  }
  network.connect();
  netState = 'connected';
  log('connected as player', VCDJ_ID);
  network.statusEmitter.on('status', onStatus);
  setInterval(() => {
    // mark decks whose status went quiet, so a WiFi gap reads as "stale", not as a stopped deck
    const now = Date.now();
    let changed = false;
    for (const d of decks.values()) {
      const stale = now - d.seen > STALE_MS;
      if (stale !== d.stale) { d.stale = stale; changed = true; }
    }
    if (changed) broadcast('decks', deckRows());
  }, 1000);
}

function currentConfig() {
  // autoconfigFromPeers picked the interface; keep it and only change our player number
  for (const list of Object.values(networkInterfaces())) {
    for (const i of list ?? []) {
      if (i.family !== 'IPv4' || i.internal) continue;
      const peer = [...network.deviceManager.devices.values()][0];
      if (peer && sameSubnet(i, peer.ip.address)) return { iface: i };
    }
  }
  throw new Error('no interface on the same subnet as the players');
}
function sameSubnet(i, ip) {
  const n = (s) => s.split('.').reduce((a, b) => (a << 8) + Number(b), 0) >>> 0;
  const m = n(i.netmask);
  return (n(i.address) & m) === (n(ip) & m);
}

// ---- decks
const PLAYING = new Set([PlayState.Playing, PlayState.Looping]);
function onStatus(s) {
  if (!s || typeof s.deviceId !== 'number') return;
  const prev = decks.get(s.deviceId);
  const loaded = s.trackId > 0 && s.trackSlot !== MediaSlot.Empty;
  const row = {
    deck: s.deviceId,
    name: network.deviceManager.devices.get(s.deviceId)?.name ?? `Player ${s.deviceId}`,
    track: loaded ? { device: s.trackDeviceId, slot: s.trackSlot, id: s.trackId, type: s.trackType } : null,
    playing: PLAYING.has(s.playState),
    on_air: !!s.isOnAir,
    play_state: PlayState[s.playState] ?? String(s.playState),
    seen: Date.now(),
    stale: false,
    meta: prev?.meta ?? null,
  };
  const key = (t) => (t ? `${t.device}:${t.slot}:${t.id}` : '');
  if (key(row.track) !== key(prev?.track)) row.meta = null;
  decks.set(s.deviceId, row);
  const changed = !prev || prev.stale || key(row.track) !== key(prev.track) || row.playing !== prev.playing || row.on_air !== prev.on_air;
  if (changed) {
    broadcast('decks', deckRows());
    if (row.track && !row.meta) describeDeck(row);
  }
}
async function describeDeck(row) {
  const t = row.track;
  try {
    const meta = await trackMeta(t.device, t.slot, t.id, false);
    const cur = decks.get(row.deck);
    if (cur?.track?.id === t.id && cur.track.device === t.device) {
      cur.meta = { title: meta.title, artist: meta.artist, duration: meta.duration, bpm: meta.bpm, playlists: meta.playlists,
        file_path: meta.file_path, file_size: meta.file_size };
      broadcast('decks', deckRows());
    }
  } catch (e) {
    log(`deck ${row.deck}: no metadata for track ${t.id}: ${e.message}`);
  }
}
const deckRows = () =>
  [...decks.values()].sort((a, b) => a.deck - b.deck).map(({ seen, ...d }) => ({ ...d, age_ms: Date.now() - seen }));

function devices() {
  if (!network) return [];
  return [...network.deviceManager.devices.values()].map((d) => ({
    id: d.id, name: d.name, type: DeviceType[d.type] ?? d.type, ip: d.ip.address,
  }));
}

// ---- media databases (export.pdb over NFS, cached by prolink-connect per device and slot)
async function orm(device, slot) {
  if (!network?.isConnected()) throw httpError(503, `network ${netState}`);
  const db = await serial(() => retry(() => network.localdb.get(device, slot), `database ${device}/${slot}`));
  if (!db) throw httpError(404, `no rekordbox database on player ${device} ${MediaSlot[slot] ?? slot}`);
  return db;
}

const playlistIndex = new Map(); // "device:slot" -> Map(trackId -> [playlist path])
async function playlistsOf(device, slot, trackId) {
  const k = `${device}:${slot}`;
  if (!playlistIndex.has(k)) {
    const db = await orm(device, slot);
    const idx = new Map();
    const walk = (id, path) => {
      const { folders, playlists, trackEntries } = db.findPlaylist(id);
      for (const e of trackEntries) {
        const l = idx.get(e.trackId) ?? [];
        l.push({ id, name: path.join(' / ') });
        idx.set(e.trackId, l);
      }
      for (const p of [...folders, ...playlists]) walk(p.id, [...path, p.name]);
    };
    walk(undefined, []);
    playlistIndex.set(k, idx);
  }
  return playlistIndex.get(k).get(trackId) ?? [];
}

const trackRow = (t) => ({
  id: t.id, title: t.title, artist: t.artist?.name ?? null, duration: t.duration,
  bpm: t.tempo ? t.tempo / 100 : null, key: t.key?.name ?? null, genre: t.genre?.name ?? null,
});

async function trackMeta(device, slot, id, withGrid) {
  const db = await orm(device, slot);
  const t = db.findTrack(id);
  if (!t || !t.id) throw httpError(404, `no track ${id}`);
  const row = { ...trackRow(t), file_path: t.filePath, file_name: t.fileName, file_size: t.fileSize ?? null,
    playlists: await playlistsOf(device, slot, id) };
  if (withGrid) {
    const full = await serial(() => retry(() =>
      network.db.getMetadata({ deviceId: device, trackSlot: slot, trackType: 1, trackId: id }), `grid ${id}`));
    const grid = full?.beatGrid ?? [];
    row.beats = grid.map((b) => Math.round(b.offset * 10) / 10000);
    row.beat_in_bar = grid.map((b) => b.count);
  }
  return row;
}

async function listPlaylist(device, slot, id) {
  const db = await orm(device, slot);
  const { folders, playlists, trackEntries } = db.findPlaylist(id);
  const tracks = trackEntries.sort((a, b) => a.sortIndex - b.sortIndex).map((e) => trackRow(db.findTrack(e.trackId)));
  const strip = (p) => ({ id: p.id, name: p.name, folder: !!p.isFolder });
  return { folders: folders.map(strip), playlists: playlists.map(strip), tracks };
}

async function audio(device, slot, id) {
  const db = await orm(device, slot);
  const t = db.findTrack(id);
  if (!t?.filePath) throw httpError(404, `no track ${id}`);
  const dev = network.deviceManager.devices.get(device);
  if (!dev) throw httpError(404, `player ${device} is gone`);
  log(`download ${t.filePath} from player ${device}`);
  const t0 = Date.now();
  const buf = await serial(() => retry(() => fetchFile({ device: dev, slot, path: t.filePath }), `download ${id}`));
  log(`downloaded ${(buf.length / 1e6).toFixed(1)} MB in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
  return { buf, name: t.fileName || t.filePath.split('/').pop() };
}

// ---- HTTP
function httpError(code, msg) { return Object.assign(new Error(msg), { code }); }
function broadcast(event, data) {
  const msg = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
  for (const res of listeners) res.write(msg);
}
const num = (q, k) => {
  const v = q.get(k);
  if (v === null || v === '') return undefined;
  const n = Number(v);
  if (!Number.isFinite(n)) throw httpError(400, `bad ${k}`);
  return n;
};

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  const q = url.searchParams;
  const json = (body, code = 200) => {
    res.writeHead(code, { 'content-type': 'application/json' });
    res.end(JSON.stringify(body));
  };
  try {
    switch (url.pathname) {
      case '/health':
        return json({ state: netState, error: netError, player: VCDJ_ID, devices: devices().length });
      case '/devices':
        return json(devices());
      case '/decks':
        return json(deckRows());
      case '/events': {
        res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'keep-alive' });
        res.write(`event: decks\ndata: ${JSON.stringify(deckRows())}\n\n`);
        listeners.add(res);
        const ping = setInterval(() => res.write(': ping\n\n'), 15000);
        req.on('close', () => { clearInterval(ping); listeners.delete(res); });
        return;
      }
      case '/playlists':
        return json(await listPlaylist(num(q, 'device'), num(q, 'slot'), num(q, 'id')));
      case '/track':
        return json(await trackMeta(num(q, 'device'), num(q, 'slot'), num(q, 'id'), q.get('grid') === '1'));
      case '/audio': {
        const { buf, name } = await audio(num(q, 'device'), num(q, 'slot'), num(q, 'id'));
        res.writeHead(200, { 'content-type': 'application/octet-stream', 'content-length': buf.length,
          'x-file-name': encodeURIComponent(name) });
        return res.end(buf);
      }
      default:
        return json({ error: 'not found' }, 404);
    }
  } catch (e) {
    log(`${url.pathname}: ${e.message}`);
    if (!res.headersSent) json({ error: e.message }, e.code ?? 500);
    else res.end();
  }
});
server.listen(PORT, '127.0.0.1', () => log(`bridge on http://127.0.0.1:${PORT}`));

start().catch((e) => {
  netState = 'failed';
  netError = e.message;
  log('network failed:', e.stack ?? e);
  // e.g. the Pro DJ Link ports are taken (another Pro DJ Link app on this Mac): exit, the server restarts us
  setTimeout(() => process.exit(1), 2000);
});
