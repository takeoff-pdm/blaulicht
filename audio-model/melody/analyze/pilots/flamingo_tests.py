"""Controls for the Music Flamingo pilot, on top of `analyze.run flamingo`. On ae-charlie, in code/:

    ../.venv-flamingo/bin/python -m analyze.pilots.flamingo_tests [sections,controls,timestamps,robust,fixed,batch,long]

Each group writes `out/pilots/flamingo/<group>.json` as it goes:
- sections: the part prompt on each clip of `show.sections`, for the function and breakdown-to-drop questions
- controls: silence, faint noise, a sine and single stems, to see what it claims about them
- timestamps: ask for the drops directly, then crop or pad the song: times taken from the audio would move
- robust: the part prompt with its options reversed and without the genre hint
- fixed: clips of equal length just before and after each drop, because energy grows with clip length
- batch: batches of 4, for the speed estimate
- long: memory on the longest song and at the 20 minute cap
"""

import json
import sys
import time

import numpy as np

from analyze import common, flamingo
from analyze.flamingo import CONTEXT, DESCRIBE, PART, SECTIONS, SR16, ask, parse_part, parse_timestamps, peak_memory, to16k

OUT = common.ROOT / "out/pilots/flamingo"
SONGS = ["58e556af6855", "fe0b3c607b1b", "8a0e3b75f4a1", "dcb7a7d3e846", "6a67050f048d"]
LONGEST = "b648767e5a1f"  # Adam Beyer & Bart Skils - Your Mind, 503 s

DROPS = "At what times do the drops occur in this track? Answer only with timestamps in the form mm:ss, one per line."
HEAR = "What instruments can you hear in this clip? If you can't hear something, don't name it. If the clip is silent, say so."
PART_REVERSED = PART.replace('"intro" or "build-up" or "drop" or "breakdown" or "outro"', '"outro" or "breakdown" or "drop" or "build-up" or "intro"')
PART_NO_HINT = PART.replace("This clip is one part of an electronic dance track. ", "")
assert PART_REVERSED != PART and PART_NO_HINT != PART
CAREFUL = "You are a careful audio analyst. Report only what is actually audible in the clip. The clip may contain no music at all."


def save(group: str, doc) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f"{group}.json").write_text(json.dumps(doc, indent=1, ensure_ascii=False))


def context(sid: str) -> dict:
    return json.loads((CONTEXT / f"{sid}.json").read_text())


def clip(x: np.ndarray, a: float, b: float) -> np.ndarray:
    return x[int(a * SR16): int(b * SR16)]


def run_sections(state, songs):
    doc = {}
    for s in songs:
        x = to16k(common.stems(s)["mix"])
        rows = []
        for sec in context(s.id)["sections"]:
            a = ask(state, clip(x, sec["start"], sec["end"]), PART)
            rows.append({**sec, **a, "parsed": parse_part(a["raw"])})
            print(s.name, sec["kind"], a["seconds"], a["raw"][:200].replace("\n", " "), flush=True)
        doc[s.id] = rows
        save("sections", doc)


def run_controls(state, songs):
    rng = np.random.default_rng(0)
    n = 30 * SR16
    clips = {
        "digital silence": np.zeros(n, np.float32),
        "white noise at -70 dBFS": (10 ** (-70 / 20) * rng.standard_normal(n)).astype(np.float32),
        "sine 440 Hz at -20 dBFS": (0.1 * np.sin(2 * np.pi * 440 * np.arange(n) / SR16)).astype(np.float32),
    }
    for s in songs:
        c = context(s.id)
        t = c["drops"][0]["time"] if c["drops"] else 60.0
        st = common.stems(s)
        for stem in ("mix", "drums") + (("bass", "vocals") if s.id in ("58e556af6855", "8a0e3b75f4a1") else ()):
            clips[f"{s.name}, {stem} only, drop {t:.0f}-{t + 30:.0f} s"] = clip(to16k(st[stem]), t, t + 30)
    doc = {}
    for name, a in clips.items():
        rows = {"rms_dbfs": round(float(10 * np.log10(np.mean(a.astype(np.float64) ** 2) + 1e-20)), 1)}
        part = ask(state, a, PART)
        rows["part"] = {**part, "parsed": parse_part(part["raw"])}
        rows["describe"] = ask(state, a, DESCRIBE, 200)
        rows["hear"] = ask(state, a, HEAR, 100)
        if "only" not in name:
            rows["hear_careful_system"] = ask(state, a, HEAR, 100, system=CAREFUL)
        doc[name] = rows
        print(name, rows["rms_dbfs"], rows["part"]["raw"][:300].replace("\n", " "), "||", rows["hear"]["raw"][:200].replace("\n", " "), flush=True)
        save("controls", doc)


def run_timestamps(state, songs):
    doc = {}
    for s in songs:
        x = to16k(common.stems(s)["mix"])
        rows = {"drops_direct": ask(state, x, DROPS, 100)}
        crop = 60
        for name, audio, shift in (("cropped_60", x[crop * SR16:], -crop), ("padded_30", np.concatenate([np.zeros(30 * SR16, np.float32), x]), 30)):
            a = ask(state, audio, SECTIONS, 800)
            d = ask(state, audio, DROPS, 100)
            rows[name] = {"shift": shift, "sections": {**a, "parsed": parse_timestamps(a["raw"])}, "drops_direct": d}
        doc[s.id] = rows
        print(s.name, rows["drops_direct"]["raw"].replace("\n", " "), "| cropped:", rows["cropped_60"]["drops_direct"]["raw"].replace("\n", " "),
              "| padded:", rows["padded_30"]["drops_direct"]["raw"].replace("\n", " "), flush=True)
        save("timestamps", doc)


def run_robust(state, songs):
    doc = {}
    for s in songs:
        x = to16k(common.stems(s)["mix"])
        rows = []
        for start, end, letter in context(s.id)["parts"]:
            a = clip(x, start, end)
            row = {"start": start, "end": end, "letter": letter}
            for name, prompt in (("reversed", PART_REVERSED), ("no_hint", PART_NO_HINT)):
                r = ask(state, a, prompt)
                row[name] = {**r, "parsed": parse_part(r["raw"])}
            rows.append(row)
        doc[s.id] = rows
        print(s.name, "robust done", flush=True)
        save("robust", doc)


def run_fixed(state, songs):
    """Energy grows with clip length and drop sections are the longest, so compare equal lengths around each drop."""
    doc = {}
    for s in songs:
        x, c = to16k(common.stems(s)["mix"]), context(s.id)
        rows = []
        for d in c["drops"]:
            t = d["time"]
            for length in (8, 16, 30):
                if t - length < 0 or t + length > c["duration"]:
                    continue
                row = {"time": t, "length": length}
                for side, a in (("before", clip(x, t - length, t)), ("after", clip(x, t, t + length))):
                    for name, prompt in (("default", PART), ("reversed", PART_REVERSED)):
                        r = ask(state, a, prompt)
                        row[f"{side}_{name}"] = {**r, "parsed": parse_part(r["raw"])}
                rows.append(row)
        doc[s.id] = rows
        print(s.name, "fixed done", flush=True)
        save("fixed", doc)


def run_batch(state, songs):
    torch, processor, model = state
    s = next(s for s in songs if s.id == SONGS[0])
    x = to16k(common.stems(s)["mix"])
    parts = context(s.id)["parts"]
    doc = []
    for size in (1, 4, 8):
        t0, texts = time.time(), []
        for i in range(0, len(parts), size):
            messages = [[{"role": "user", "content": [{"type": "text", "text": PART}, {"type": "audio", "audio": flamingo.fit(clip(x, a, b))}]}]
                        for a, b, _ in parts[i: i + size]]
            # no padding=True here: it would reach the feature extractor too, which must pad each window to 30 s
            inputs = processor.apply_chat_template(messages, tokenize=True, add_generation_prompt=True, return_dict=True).to(model.device)
            inputs["input_features"] = inputs["input_features"].to(model.dtype)
            with torch.inference_mode():
                out = model.generate(**inputs, max_new_tokens=160, do_sample=False)
            texts += processor.batch_decode(out[:, inputs.input_ids.shape[1]:], skip_special_tokens=True)
        row = {"batch": size, "clips": len(parts), "seconds": round(time.time() - t0, 1), "same_as_batch_1": None, "raw": [t.strip() for t in texts]}
        if doc:
            row["same_as_batch_1"] = sum(t == r for t, r in zip(row["raw"], doc[0]["raw"]))
        doc.append(row)
        print("batch", size, row["seconds"], row["same_as_batch_1"], flush=True)
        save("batch", doc)


def run_long(state, songs):
    torch = state[0]
    doc = {}
    longest = next(s for s in common.songs() if s.id == LONGEST)
    x = to16k(common.stems(longest)["mix"])
    cases = {"longest song, 503 s": x, "20 minutes, the cap": np.tile(x, 1 + 1200 * SR16 // len(x))[:1200 * SR16]}
    for name, a in cases.items():
        torch.cuda.empty_cache()
        torch.cuda.reset_peak_memory_stats()
        try:
            r = ask(state, a, SECTIONS, 100)
            doc[name] = {"seconds": r["seconds"], **peak_memory(state), "raw": r["raw"]}
        except torch.cuda.OutOfMemoryError as e:
            doc[name] = {"error": str(e)[:300], **peak_memory(state)}
        print(name, doc[name], flush=True)
        save("long", doc)


def main():
    groups = (sys.argv[1] if len(sys.argv) > 1 else "sections,controls,timestamps,robust,fixed,batch,long").split(",")
    songs = [s for s in common.songs() if s.id in SONGS]
    state = flamingo.setup()
    print("loaded", peak_memory(state), flush=True)
    for g in groups:
        t0 = time.time()
        state[0].cuda.reset_peak_memory_stats()
        globals()[f"run_{g}"](state, songs)
        print(f"== {g}: {time.time() - t0:.0f} s, {peak_memory(state)}", flush=True)


if __name__ == "__main__":
    main()
