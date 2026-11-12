"""Job state in SQLite, so the queue survives a restart. Standard library only.

A job is one track from one player's media. `song_id` is the pipeline's id (see pipeline/song.py
song_id), known once the audio is downloaded. `known` remembers which file on a stick became which
song, so a track that was analysed once is never downloaded again, even from another player.
"""

import sqlite3
import threading
import time
from pathlib import Path

# queued -> downloading -> waiting -> analysing -> compiling -> ready; failed and cancelled end a job
ACTIVE = ("queued", "downloading", "waiting", "analysing", "compiling")
SCHEMA = """
create table if not exists jobs (
  id integer primary key,
  device integer not null, slot integer not null, track_id integer not null,
  title text, artist text, duration real,
  file_path text, file_size integer,
  playlist text, position integer default 0,
  song_id text, listed text,
  state text not null default 'queued', step text, steps_done integer, steps_total integer,
  worker text, error text, bump integer default 0,
  created real not null, updated real not null
);
create index if not exists jobs_track on jobs(device, slot, track_id);
create table if not exists known (
  file_path text not null, file_size integer not null, song_id text not null, listed text not null,
  primary key (file_path, file_size)
);
"""


class Store:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
        self.db.row_factory = sqlite3.Row
        self.lock = threading.Lock()
        with self.lock:
            self.db.executescript(SCHEMA)
            # a restart interrupts downloads and compiles; analyses on ae-charlie keep running and are followed again
            self.db.execute("update jobs set state='queued', step=null where state='downloading'")
            self.db.execute("update jobs set state='waiting', step=null where state='compiling'")

    def q(self, sql: str, *args) -> list[dict]:
        with self.lock:
            return [dict(r) for r in self.db.execute(sql, args).fetchall()]

    def one(self, sql: str, *args) -> dict | None:
        rows = self.q(sql, *args)
        return rows[0] if rows else None

    def add(self, device: int, slot: int, track: dict, playlist: str | None, position: int) -> dict:
        """A job for this track, unless one is active or ready already (then that one is returned)."""
        old = self.one("select * from jobs where device=? and slot=? and track_id=? and state in (%s, 'ready') order by id desc"
                       % ",".join("?" * len(ACTIVE)), device, slot, track["id"], *ACTIVE)
        if old:
            return old
        now = time.time()
        with self.lock:
            cur = self.db.execute(
                "insert into jobs (device, slot, track_id, title, artist, duration, playlist, position, created, updated)"
                " values (?,?,?,?,?,?,?,?,?,?)",
                (device, slot, track["id"], track.get("title"), track.get("artist"), track.get("duration"), playlist, position, now, now))
            jid = cur.lastrowid
        return self.get(jid)

    def get(self, jid: int) -> dict | None:
        return self.one("select * from jobs where id=?", jid)

    def update(self, jid: int, **fields) -> None:
        fields["updated"] = time.time()
        cols = ", ".join(f"{k}=?" for k in fields)
        with self.lock:
            self.db.execute(f"update jobs set {cols} where id=?", (*fields.values(), jid))

    def jobs(self, limit: int = 500) -> list[dict]:
        return self.q("select * from jobs order by id desc limit ?", limit)

    def in_state(self, *states: str) -> list[dict]:
        return self.q("select * from jobs where state in (%s) order by id" % ",".join("?" * len(states)), *states)

    def known(self, file_path: str, file_size: int | None) -> dict | None:
        return self.one("select * from known where file_path=? and file_size=?", file_path, file_size or 0)

    def remember(self, file_path: str, file_size: int | None, song_id: str, listed: str) -> None:
        with self.lock:
            self.db.execute("insert or replace into known values (?,?,?,?)", (file_path, file_size or 0, song_id, listed))

    def by_track(self, device: int, slot: int, track_id: int) -> dict | None:
        return self.one("select * from jobs where device=? and slot=? and track_id=? and state != 'cancelled' order by id desc",
                        device, slot, track_id)
