"""How well a fine-tuned checkpoint agrees with the labels on the hold-out songs. Runs on ae-charlie.

    CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0 ../.venv-sf/bin/python -m analyze.edm_finetune.evaluate \
        --labels weak --run dryrun [--checkpoint base|last|model.ckpt-800.pt]

Prints and writes out/edm_finetune/<labels>_<run>/report.json and pred/<id>.json (segments and the frame
labels). Compared with stock SongFormer and All-In-One on the same songs, their pop labels mapped to
the EDM ones by majority overlap on the songs outside the hold-out, and with always saying "drop".

With --labels weak the labels ARE the rule, so a high score means the model can learn the rule, not that
it is better than the rule. A disagreement can be the model's mistake or the rule's: only a listener can say.
"""

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np

from analyze import common
from analyze.edm_finetune import train as tr

EDM_LABELS = ["intro", "buildup", "drop", "breakdown", "outro", "silence"]
GROOVE = "NO_LABEL"
FPS = 12.5
WINDOWS = (12, 4)  # peak-picking window in seconds either side: upstream's, and short enough for 4-bar builds


def load_model(out: Path, which: str):
    import torch
    from ema_pytorch import EMA
    from models.SongFormer import Model
    from omegaconf import OmegaConf

    hp = OmegaConf.load(out / "config.yaml")
    model = Model(hp)
    if which == "base":
        name = "base.pt"
    elif which == "best":
        name = (out / "best.txt").read_text().strip()
    elif which == "last":
        name = (out / "checkpoint").read_text().strip()
    else:
        name = which
    ck = torch.load(out / name, map_location="cpu", weights_only=False)
    if "model_ema" in ck:
        ema = EMA(model, include_online_model=False, **hp.ema_kwargs)
        ema.load_state_dict(ck["model_ema"])
        model.load_state_dict(ema.ema_model.state_dict())
    else:
        model.load_state_dict(ck["model"])
    return model.cuda().eval(), hp, name


def predict(model, song_id: str):
    """Frame logits of the whole song at 12.5 fps: (frames, 6) over EDM_LABELS, (frames) boundaries."""
    import torch
    from dataset.label2id import LABEL_TO_ID

    allowed = [LABEL_TO_ID[x] for x in EDM_LABELS]
    starts = sorted(int(p.stem.rsplit("_", 1)[1]) for p in (tr.EMBEDDINGS / tr.SETS[0]).glob(f"{song_id}_*.npy"))
    func, bound = [], []
    ids = torch.tensor([tr.DATASET_ID]).cuda()
    with torch.no_grad():
        for start in starts:
            parts = [np.load(tr.EMBEDDINGS / name / f"{song_id}_{start}.npy") for name in tr.SETS]
            x = torch.from_numpy(np.concatenate(parts, -1)).float().cuda()
            x = model.down_sample_conv(model.input_norm(model.mixed_win_downsample(x)))
            x = model.AddFuse(x=x, cond=model.dataset_class_prefix(ids).unsqueeze(1))
            x = model.transformer(x=x, src_key_padding_mask=None)
            f, b = model.function_head(x)[0, :, allowed].cpu(), model.boundary_head(x)[0, :, 0].cpu()
            want = int(start * FPS)
            have = sum(len(y) for y in func)
            if want > have and func:  # 420 s gives 5249 frames: repeat the last one over the seam
                func.append(func[-1][-1:].repeat(want - have, 1))
                bound.append(bound[-1][-1:].repeat(want - have))
            func.append(f)
            bound.append(b)
    return torch.cat(func), torch.cat(bound)


def segments_from(func, bound, window_s: float, filter_size: int):
    """Upstream's post-processing (postprocessing/functional.py) with the peak window as a parameter."""
    import torch
    from postprocessing.helpers import local_maxima, peak_picking

    prob = torch.sigmoid(bound)
    peaks, _ = local_maxima(prob, filter_size=filter_size)
    n = int(window_s * FPS)
    strength = peak_picking(peaks.numpy(), window_past=n, window_future=n)
    cuts = [0] + [int(i) for i in np.flatnonzero(strength > 0) if i > 0] + [len(prob)]
    p = torch.softmax(func, -1).numpy()
    segs = []
    for a, b in zip(cuts[:-1], cuts[1:]):
        segs.append([round(a / FPS, 2), round(b / FPS, 2), EDM_LABELS[int(p[a:b].mean(0).argmax())]])
    return merged(segs)


def merged(segs):
    out = []
    for a, b, label in segs:
        if out and out[-1][2] == label:
            out[-1][1] = b
        else:
            out.append([a, b, label])
    return out


def per_frame(segs, n: int) -> list:
    """The label of each of n frames at 12.5 fps; None where no segment covers it."""
    out = [None] * n
    for a, b, label in segs:
        for i in range(max(0, int(a * FPS)), min(n, int(b * FPS))):
            out[i] = label
    return out


def hits(ref, est, tol: float) -> int:
    """One-to-one matches between two sorted time lists within tol."""
    ref, est = sorted(ref), sorted(est)
    used, n = set(), 0
    for t in ref:
        best = None
        for j, e in enumerate(est):
            if j not in used and abs(e - t) <= tol and (best is None or abs(e - t) < abs(est[best] - t)):
                best = j
        if best is not None:
            used.add(best)
            n += 1
    return n


def inner_boundaries(segs):
    return [s[0] for s in segs if s[0] > 0.5]


def boundary_scores(rows, tol_of):
    """rows: (ref boundaries, est boundaries, song). Pooled precision, recall and F."""
    tp = sum(hits(r, e, tol_of(s)) for r, e, s in rows)
    nr, ne = sum(len(r) for r, _, _ in rows), sum(len(e) for _, e, _ in rows)
    p, r = tp / max(1, ne), tp / max(1, nr)
    return {"precision": round(p, 3), "recall": round(r, 3), "f": round(2 * p * r / max(1e-9, p + r), 3)}


def label_map_baseline(name: str, reference: dict, train_ids, test_ids):
    """Pop labels to EDM labels by majority overlap outside the hold-out; agreement on the hold-out."""
    def segs(i):
        p = common.FEATURES / name / f"{i}.json"
        return json.loads(p.read_text()).get("segments") or [] if p.exists() else []

    overlap = defaultdict(Counter)
    for i in train_ids:
        n = int(reference[i]["duration"] * FPS)
        ref = per_frame(reference[i]["sections"], n)
        for lab, r in zip(per_frame(segs(i), n), ref):
            if lab is not None and r not in (None, GROOVE):
                overlap[lab][r] += 1
    mapping = {k: v.most_common(1)[0][0] for k, v in overlap.items()}
    good, total, used = 0, 0, 0
    for i in test_ids:
        s = segs(i)
        if not s:
            continue
        used += 1
        n = int(reference[i]["duration"] * FPS)
        for lab, r in zip(per_frame(s, n), per_frame(reference[i]["sections"], n)):
            if r not in (None, GROOVE):
                total += 1
                good += mapping.get(lab) == r
    return {"agreement": round(good / max(1, total), 3), "mapping": mapping, "songs": used}


def chance_drop_on_boundary(rows, rng, tol_of, shifts=50):
    """Mean share of drops within tol of a boundary if the song's boundaries are slid round by a random
    amount: same number and spacing of boundaries, unrelated to the music."""
    out = []
    for drops, est, s in rows:
        if not drops or not est:
            continue
        dur, tol = s["duration"], tol_of(s)
        est = np.array(est)
        vals = []
        for _ in range(shifts):
            e = (est + rng.uniform(0, dur)) % dur
            vals.append(np.mean([np.abs(e - d).min() <= tol for d in drops]))
        out.append(np.mean(vals))
    return round(float(np.mean(out)), 3) if out else None


def disagreements(song, ref_frames, est_frames, min_s: float):
    """Runs of at least min_s seconds where the model and the labels differ, longest first."""
    out, i, n = [], 0, len(ref_frames)
    while i < n:
        r, e = ref_frames[i], est_frames[i]
        if r in (None, GROOVE) or r == e or e is None:
            i += 1
            continue
        j = i
        while j < n and ref_frames[j] == r and est_frames[j] == e:
            j += 1
        if (j - i) / FPS >= min_s:
            out.append({"song": song["name"], "id": song["id"], "start": round(i / FPS, 1), "end": round(j / FPS, 1), "labels": r, "model": e})
        i = j
    return sorted(out, key=lambda d: d["start"] - d["end"])


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--checkpoint", default="best", help="best, last, base (stock weights, untrained EDM labels) or a file name")
    ap.add_argument("--split", default="holdout")
    ap.add_argument("--model-dir", type=Path, help="default models/edm_finetune/<labels>_<run>")
    ap.add_argument("--out", type=Path, help="default out/edm_finetune/<labels>_<run>")
    ap.add_argument("--data", type=Path, help="default data/<labels> next to this file")
    args = ap.parse_args()

    tag = f"{args.labels}_{args.run}"
    model_dir = args.model_dir or common.ROOT / "models/edm_finetune" / tag
    out = args.out or common.ROOT / "out/edm_finetune" / tag
    (out / "pred").mkdir(parents=True, exist_ok=True)
    data = args.data or tr.HERE / "data" / args.labels
    reference = json.loads((data / "reference.json").read_text())
    read_ids = lambda name: [x for x in (data / f"{name}.txt").read_text().split() if x in reference]
    test_ids, other_ids = read_ids(args.split), read_ids("train") + read_ids("val")
    left_out = [i for i, r in reference.items() if "left_out" in r]

    tr.add_upstream_to_path()
    model, hp, ck_name = load_model(model_dir, args.checkpoint)
    rng = np.random.default_rng(7)
    tol_beats = lambda s: 2 * s["beat"] if s.get("beat") else 1.0

    good = Counter()
    total = Counter()
    per_song_acc = []
    confusion = {w: defaultdict(Counter) for w in WINDOWS}
    frame_confusion = defaultdict(Counter)
    rows = {w: [] for w in WINDOWS}
    drop_rows = {w: [] for w in WINDOWS}
    examples = []
    label_share = Counter()
    seg_counts = {w: [] for w in WINDOWS}
    bar_hits = {w: [0, 0] for w in WINDOWS}
    bar_rule = [0, 0]
    drop_label = {w: {"rule_drops": 0, "found": 0, "model_drops": 0, "confirmed": 0} for w in WINDOWS}
    rule_bounds = []

    for i in test_ids:
        s = reference[i]
        func, bound = predict(model, i)
        n = len(bound)
        ref = per_frame(s["sections"], n)
        frame_labels = [EDM_LABELS[k] for k in func.argmax(-1).tolist()]
        ref_bounds = inner_boundaries(s["sections"])
        rule_bounds.append((ref_bounds, s))
        pred = {"id": i, "name": s["name"], "frame_labels_fps": FPS, "windows": {}}
        song_good, song_total = 0, 0
        for f, r in zip(frame_labels, ref):
            if r is None:
                continue
            frame_confusion[r][f] += 1
            if r != GROOVE:
                total["frame"] += 1
                good["frame"] += f == r
                label_share[r] += 1
        for w in WINDOWS:
            segs = segments_from(func, bound, w, hp.local_maxima_filter_size)
            pred["windows"][str(w)] = segs
            est = per_frame(segs, n)
            for e, r in zip(est, ref):
                if r is None or e is None:
                    continue
                confusion[w][r][e] += 1
                if r != GROOVE:
                    total[f"seg{w}"] += 1
                    good[f"seg{w}"] += e == r
                    if w == WINDOWS[-1]:
                        song_total += 1
                        song_good += e == r
            starts = inner_boundaries(segs)
            rows[w].append((ref_bounds, starts, s))
            drop_rows[w].append((s["drops"], starts, s))
            seg_counts[w].append(len(segs))
            down = np.array(s.get("downbeats") or [])
            if len(down) and s.get("beat"):
                bar_hits[w][1] += len(starts)
                bar_hits[w][0] += sum(np.abs(down - t).min() <= 0.25 * s["beat"] for t in starts)
            tol = tol_beats(s)
            model_drops = [a for a, _, label in segs if label == "drop"]
            d = drop_label[w]
            d["rule_drops"] += len(s["drops"])
            d["model_drops"] += len(model_drops)
            d["found"] += sum(any(abs(a - t) <= tol for a in model_drops) for t in s["drops"])
            d["confirmed"] += sum(any(abs(a - t) <= tol for t in s["drops"]) for a in model_drops)
        examples += disagreements(s, ref, per_frame(pred["windows"][str(WINDOWS[-1])], n), 8.0)
        per_song_acc.append(song_good / max(1, song_total))
        (out / "pred" / f"{i}.json").write_text(json.dumps(pred, ensure_ascii=False, separators=(",", ":")))

    report = {"labels": args.labels, "checkpoint": ck_name, "songs": len(test_ids), "agreement": {}, "boundaries": {}, "drops": {}, "confusion": {}}
    # always the most common label, which needs no model
    report["agreement"]["always_most_common"] = {"label": label_share.most_common(1)[0][0], "value": round(label_share.most_common(1)[0][1] / max(1, sum(label_share.values())), 3)}
    report["agreement"]["model_frames"] = round(good["frame"] / max(1, total["frame"]), 3)
    for w in WINDOWS:
        report["agreement"][f"model_segments_peak{w}s"] = round(good[f"seg{w}"] / max(1, total[f"seg{w}"]), 3)
    report["agreement"]["median_per_song_peak4s"] = round(float(np.median(per_song_acc)), 3)
    report["agreement"]["stock_songformer_mapped"] = label_map_baseline("songformer", reference, other_ids, test_ids)
    report["agreement"]["allinone_mapped"] = label_map_baseline("allinone", reference, other_ids, test_ids)

    def stock_rows(name):
        out_rows = []
        for ref_bounds, s in rule_bounds:
            p = common.FEATURES / name / f"{s['id']}.json"
            if p.exists():
                segs = [x for x in json.loads(p.read_text()).get("segments") or [] if x[2] not in ("start", "end")]
                out_rows.append((ref_bounds, inner_boundaries(segs), s))
        return out_rows

    tolerances = {"0.5s": lambda s: 0.5, "3s": lambda s: 3.0, "2beats": tol_beats}
    def boundary_block(rs):
        block = {k: boundary_scores(rs, f) for k, f in tolerances.items()}
        block["segments_median"] = float(np.median([len(e) + 1 for _, e, _ in rs])) if rs else None
        return block

    for w in WINDOWS:
        report["boundaries"][f"model_peak{w}s"] = boundary_block(rows[w])
        d = drop_label[w]
        report["drops"][f"model_peak{w}s"] = {
            **d,
            "rule_drops_with_a_model_drop_start": round(d["found"] / max(1, d["rule_drops"]), 3),
            "model_drop_starts_on_a_rule_drop": round(d["confirmed"] / max(1, d["model_drops"]), 3),
            "rule_drops_on_any_boundary": round(float(np.mean([np.mean([np.abs(np.array(e) - t).min() <= tol_beats(s) for t in dr]) for dr, e, s in drop_rows[w] if dr and e])), 3),
            "chance_rule_drops_on_any_boundary": chance_drop_on_boundary(drop_rows[w], rng, tol_beats),
            "on_a_bar_line": round(bar_hits[w][0] / max(1, bar_hits[w][1]), 3),
        }
    for name in ("songformer", "allinone"):
        rs = stock_rows(name)
        report["boundaries"][name] = boundary_block(rs)
        dr = [(s["drops"], e, s) for _, e, s in rs]
        report["drops"][name] = {
            "rule_drops_on_any_boundary": round(float(np.mean([np.mean([np.abs(np.array(e) - t).min() <= tol_beats(s) for t in d]) for d, e, s in dr if d and e])), 3),
            "chance_rule_drops_on_any_boundary": chance_drop_on_boundary(dr, rng, tol_beats),
        }

    def table(conf):
        cols = EDM_LABELS
        out_t = {}
        for r in ["intro", "buildup", "drop", "breakdown", "outro", "silence", GROOVE]:
            row = conf.get(r)
            if row:
                n = sum(row.values())
                out_t["groove (no label)" if r == GROOVE else r] = {"seconds": round(n / FPS), **{c: round(row[c] / n, 2) for c in cols}}
        return out_t

    report["confusion"]["frames"] = table(frame_confusion)
    for w in WINDOWS:
        report["confusion"][f"segments_peak{w}s"] = table(confusion[w])

    examples.sort(key=lambda d: (not ("drop" in (d["labels"], d["model"])), d["start"] - d["end"]))
    seen = Counter()
    report["disagreements"] = []
    for e in examples:
        if seen[e["id"]] < 2:
            seen[e["id"]] += 1
            report["disagreements"].append(e)
        if len(report["disagreements"]) >= 15:
            break

    # songs the rule finds no drop in: not trained on, not scored, worth a listen
    report["no_rule_drop_songs"] = []
    for i in left_out:
        if not (tr.EMBEDDINGS / tr.SETS[0] / f"{i}_0.npy").exists():
            continue
        func, bound = predict(model, i)
        segs = segments_from(func, bound, WINDOWS[-1], hp.local_maxima_filter_size)
        report["no_rule_drop_songs"].append({"song": reference[i]["name"], "model": segs, "rule": reference[i]["sections"]})

    (out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=1))
    print(json.dumps(report, ensure_ascii=False, indent=1))
    print("wrote", out / "report.json", flush=True)


if __name__ == "__main__":
    main()
