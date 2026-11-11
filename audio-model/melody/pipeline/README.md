# From Rekordbox to the viewer, one song at a time

The viewer (`viz/`) plays a compiled song and interprets it: `serve.py` hands out files, and the looks in `club.js` and `club_looks.mjs` turn them into light. Everything that works out what's in a song lives here.

| Step | Runs on | What happens | Output in `audio-model/data/melody/` |
|---|---|---|---|
| **Pull** | Mac | One song's grid, key and phrases from a copy of Rekordbox's database | `viz/rekordbox.json` |
| **Analyze** | ae-charlie, GPU | Stems, melody lines, 17 analyzers. Slow: skips finished steps | `viz/<id>.json`, `features/<analyzer>/<id>.json` |
| **Compile** | Mac, no GPU | Grid, sections and drops, melody line, cues, checks. All 300 songs take a few seconds | `compiled/<id>.json`, `compiled/index.json` |
| **Check** (optional) | Mac | A person's labels and corrections go in, an LLM's flags get listed, then compile runs again | `labels/`, `corrections/`, `reviews/` |
| **Play** | Browser | Loads the compiled file plus the audio | |

## The page

    python3 audio-model/melody/pipeline/ui.py --open          # the compiler: http://127.0.0.1:8767
    python3 audio-model/melody/viz/serve.py --port 8766       # the viewer: no restart needed after compiling

Search Rekordbox and add a song, follow it through ae-charlie, browse the library and filter it to what needs a look, jump from a flag to that moment in the viewer, correct a grid, recompile, or start an LLM review. Nothing else is needed for day-to-day use.

The page runs the commands below as separate processes, so both ways do exactly the same thing. Its server never logs in to ae-charlie: it only asks ssh locally whether the gateway is open, and the `song.py` process following a job does the rest. If the page's server stops, the job carries on and **Follow** picks it up again.

## Commands

    alias song='python3 audio-model/melody/pipeline/song.py'
    song find "stereo love"          # search Rekordbox: id, BPM, analysed or new, path
    song add "stereo love" --wait    # pull, analyze, fetch, compile. Without --wait it returns once the job is running
    song status "stereo love"        # one song: where it is, what the checks found
    song fetch "stereo love"         # after a job without --wait: bring the results back and compile
    song analyze "stereo love"       # start or resume a pulled song, for example after the gateway was closed
    song follow "stereo love"        # wait for a running job, then fetch and compile

    song compile --stale             # after changing the compiler or labelling songs. Also: a song, or --all
    song status                      # the library: fresh or stale, grid sources, songs to check
    song status --flags              # every song with something to check, and what
    song fix "sandstorm" --bar-shift 1 --note "bars start on the kick"
    song fix "sandstorm" --clear
    song review "gravity"            # an LLM's flags on a compiled song. Costs a few cents

A song is an id, or a piece of its name if that's unique. `song add --extra` also runs the analyzers that only feed viewer lanes (`btc`, `musicfm`, `mulan`, `mt3`).

## Docker

None of this runs in Docker. Compile and the page only need Python 3.10 or newer. Pull needs `uv` and Rekordbox's database on the Mac, and analysis uses the separate environments on ae-charlie (`../charlie/envs/`).

## ae-charlie

`song` reaches ae-charlie through the gateway, which only you can open, with `ssh -fN ssh-proxy-gorilla`. A login through a closed gateway counts as a failed login on the HPI account, so `song` checks the gateway first and stops at the first connection failure. Then the job says "waiting for ae-charlie", and you run `song analyze <song>` once it's open. A running job carries on without the gateway. Its logs are in `logs/songs/<id>/` on ae-charlie.

Songs there run one at a time, each step on the GPU with the least memory in use at that moment. `remote_song.py` is the job. Stems, Melodia and the viewer's data have to succeed. A failed analyzer is noted and the rest go on, because every analyzer is optional for the compiler; the checks list what's missing.

Song ids stay what they were: the first 12 hex digits of the SHA-1 of the audio's path on ae-charlie.

## Compile

`compile_song.py` runs `grid.py`, `sections.py`, `melody_line.py`, `show_compile.py` and `checks.py` and writes one file with everything the viewer and the decision tools need. `compiled` in it records a hash of the compiler's code and of the song's inputs, which is how `song status` and `song compile --stale` tell a file has gone stale. A Sounds Like list can stay behind when other songs are added, until the next `--all`.

## Check

**Automatic checks** (`checks.py`) run on every compile and change nothing. They flag: a weak or unchecked grid; both beat trackers starting the bar elsewhere; a drop with no kick on it; no drop in an electronic song; an outro over 30% of the song; analyzers that are missing. On the 175 electronic songs, 57 have at least one flag. Those limits were set on this library and haven't been tested against hand labels, so a flag means "worth a look", not "wrong".

**Hand labels** come from the viewer's labelling mode. Once a song's status is `done`, its sections and drops replace the rule's, with drops on the nearest bar. Partial labels aren't applied.

**Corrections** (`song fix`) are for the grid: `--grid rekordbox|trackers` overrides the automatic choice, `--shift-16ths N` moves it, `--bar-shift N` starts bars later. They live in `corrections/<id>.json`, so a recompile can't lose them.

**The LLM** (`review_llm.py`) reads the song's numbers and writes flags to `reviews/<id>.json`. It can't hear the song and changes nothing: its flags appear next to the automatic ones for a person to judge.
