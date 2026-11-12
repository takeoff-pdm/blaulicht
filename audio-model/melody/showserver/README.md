# Show server

Runs on the Mac during a set. It reads the rekordbox playlists on the players over Pro DJ Link,
queues tracks for analysis, and serves the compiled shows to consumers over HTTP.

    uv run audio-model/melody/showserver/server.py            # page: http://<mac>:8770
        [--mac-worker] [--iface en0] [--player 7] [--port 8770] [--host 0.0.0.0]

The analysis needs the ae-charlie gateway: `ssh -fN ssh-proxy-gorilla`. Jobs wait while it's closed.

## How it works

- **`prolink-bridge/bridge.mjs`** (Node, `prolink-connect`) is started by the server. It joins the link as a
  passive virtual player (number 7 by default; keep it clear of the real players and of blaulicht's Pro DJ
  Link plugin). It reports which track is on each deck, whether it's playing and on air, and which playlists
  on that stick contain it. There's no beat timing, so it works over WiFi. A deck without status for 5 s is
  shown as *stale*, not stopped. Playlists, metadata, beat grids and audio come from rekordbox-exported USB/SD
  media, through the stick's `export.pdb` and NFS. NFS transfers run one at a time, with retries.
- **Queue** (`store.py`, SQLite in `data/melody/showserver.db`, survives restarts):
  `queued → downloading → waiting → analysing → compiling → ready` (or `failed` / `cancelled`).
  - Order: track on air → playing → loaded on a deck → moved to the front (↑) → order queued.
  - **One download from the players at a time.** Up to 3 downloaded songs wait for a worker before the next
    download starts; deck tracks always go.
  - The audio lands in `data/melody/prolink/<sha1>/<file>`. The same file is never downloaded or analysed twice.
- **Workers** (`workers.py`):
  - `ae-charlie` runs `song.py`'s start/fetch path, 2 songs at once by default (`remote_song.py --slots`).
  - `mac` (on with `--mac-worker`, or in the page header) runs `mac/mac_song.py` as a child process, one song at
    a time, and only when ae-charlie is full or unreachable. Stems, melodia, GAME, lead/bass, the 17 analyzers
    and the viewer data run one process at a time in the `~/work/ane-lyrics` venvs (Core ML for stems, vocals,
    drum parts, GAME and lyrics); results go to the same folders as ae-charlie's, and the same compile runs.
    Memory guard: a step starts at >= 35% free memory and load < 8, and is killed (failing the song) under 12%
    free or when swap grows 3 GB during the step (`MAC_MIN_FREE_START`, `MAC_MIN_FREE_RUN`,
    `MAC_MAX_SWAP_GROWTH_MB`, `MAC_MAX_LOAD`). Step logs: `data/melody/mac_jobs/<id>.logs/`. Stopping the server
    kills the child. Not yet timed on a whole song; expect roughly 4-5 min per song.
  - Worker on/off and songs-at-once can be set in the page header.
- **Compile** is `song.py`'s `compile_songs`, one at a time. Output: `data/melody/compiled/<song_id>.json`,
  the same file `song.py compile` writes.

## API

| | |
|---|---|
| `GET /api/decks` | per deck: `track` {device, slot, id}, `playing`, `on_air`, `stale`, `meta` {title, artist, bpm, playlists}, `song_id`, `show_ready`, `show_url` |
| `GET /api/events` | server-sent events `decks`, `jobs`, `health` on every change |
| `GET /api/shows` · `GET /api/shows/{song_id}` | finished shows; one compiled show (the compiler's JSON) |
| `GET /api/devices` · `GET /api/browse?device&slot[&id]` | players and their media; a folder or playlist, tracks with their job |
| `POST /api/queue` | `{device, slot, playlist_id, playlist}` for a whole playlist in order, or `{device, slot, track_ids: [...]}` |
| `GET /api/jobs` · `POST /api/jobs/{id}/bump\|cancel\|retry` | the queue |
| `GET /api/workers` · `PUT /api/workers/{name}` | `{enabled, concurrency}` |

## For a consumer

Watch `/api/events` (or poll `/api/decks`). Take the deck that's `on_air` and `playing`. When its `show_ready`
is true, fetch `show_url` once. The show's cue times are seconds in the track's own audio. Position and
tempo come from the consumer's own beat source (blaulicht's Pro DJ Link plugin on a wired link, for example),
because this server doesn't track the play position on purpose.

## Caveats

- rekordbox on a laptop shares its library only in Export mode. Players reading USB/SD always share theirs.
- Unanalysed files and CDs can't be read; that would need a player number 1-6 and dbserver queries.
- Cancelling a song that's already on ae-charlie stops following it, but it finishes there.
- Don't plug a WiFi router that hands out addresses (DHCP) into the booth switch. Use an access point in bridge mode.
