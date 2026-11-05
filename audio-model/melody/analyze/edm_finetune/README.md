# EDM fine-tune of SongFormer

Retrains SongFormer on the EDM labels `intro, buildup, drop, breakdown, outro, silence`, through
EDMFormer's training loop (github.com/25ohms/EDMFormer, arXiv 2603.08759). EDMFormer releases no weights,
so this starts from the stock SongFormer ones.

**Status: the weak-label dry run works end to end; no hand labels exist yet.** The `hand` path has only run
on fake files (build, 4 training steps, evaluation).

## Steps

    # Mac, from the repo root
    python3 audio-model/melody/analyze/edm_finetune/build_dataset.py --labels weak    # or: hand
    sh audio-model/melody/analyze/sync.sh push

    # ae-charlie, in code/, with CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0
    ../.venv-sf/bin/python -m analyze.edm_finetune.embed --labels weak                 # 728 s, 175 songs, 18 GB
    ../.venv-sf/bin/python -m analyze.edm_finetune.train --labels weak --run dryrun --max-steps 1000  # 42 min
    ../.venv-sf/bin/python -m analyze.edm_finetune.evaluate --labels weak --run dryrun

`.venv-sf` is Python 3.10 with `tmp/songformer/req1.txt` on ae-charlie (EDMFormer's pins for what training
imports, without wandb and mlflow, which accelerate then skips) plus PyPDF2. Install torch 2.11.0,
torchaudio 2.11.0 and torchvision 0.26.0 from cu128 last. Clones in `models/src/`, the Hugging Face repo
in `models/songformer_hf/`.

Give each job the GPU alone: MusicFM's 420 s window takes about 25 GB and training about 27 GB. Sharing
GPU 0 cost 112 songs an out-of-memory error in the `songformer` run; running it again fixed them.

## Dry run on the weak labels

1000 steps (59 epochs), 128 training songs, 33 hold-out songs not used for anything before this. Checkpoint:
`models/edm_finetune/weak_dryrun/model.ckpt-1000.pt`. Report and predictions: `out/edm_finetune/weak_dryrun/`.

| Agreement with the rule, by time (Groove left out) | |
|---|---|
| fine-tuned, peak window 4 s / upstream's 12 s | 81.6% / 74.7% |
| All-In-One / stock SongFormer, pop labels mapped by majority on the training songs | 55.4% / 51.8% |
| always "drop" / stock weights with the untrained EDM labels | 39.3% / 28.7% |

| Rule's boundaries, F at +-3 s / +-0.5 s | | Rule's drops on any boundary (chance) |
|---|---|---|
| fine-tuned, 4 s | 0.765 / 0.621 | 70% (5%) |
| fine-tuned, 12 s | 0.545 / 0.426 | 52% (3%) |
| All-In-One | 0.493 / 0.377 | 81% (10%) |
| stock SongFormer | 0.453 / 0.362 | 74% (12%) |

Share of each rule label's time, by the model's label (4 s):

| rule \ model | intro | buildup | drop | breakdown | outro |
|---|---|---|---|---|---|
| intro | 0.88 | 0.02 | 0.07 | 0.03 | 0.00 |
| buildup | 0.06 | 0.79 | 0.07 | 0.04 | 0.05 |
| drop | 0.02 | 0.00 | 0.88 | 0.00 | 0.10 |
| breakdown | 0.06 | 0.01 | 0.01 | 0.91 | 0.00 |
| outro | 0.01 | 0.03 | 0.30 | 0.09 | 0.57 |
| groove (no label) | 0.06 | 0.01 | 0.84 | 0.05 | 0.03 |

This shows the pipeline runs and the model can learn most of the rule from audio on songs it hasn't
seen. It doesn't show it is better than the rule, because the labels are the rule. The 4 s window is a
choice made here, compared on the hold-out songs, so its edge over 12 s is a little flattered. With upstream's 12 s,
buildups almost vanish (6% against 79%), as the peak picking predicts. The last row was never taught.

To check by ear (rule against model, seconds):

- Robin S - Show Me Love: rule outro from 18 to the end; model drop 10-182, outro after.
- Boris Brejcha - Gravity: rule outro 170-482; model has more drops at 311, 353, 391, 452.
- DJ Heartstring - Take My Hand (Extended): rule outro from 160; model drop 147-219.
- Dimitri Vegas & Like Mike - Mammoth: rule drop 285-332; model outro. Rule intro to 68; model drop from 45.
- Wilkinson - Afterglow, where the rule finds no drop: model drops at 33 and 110.

## Labels

- `weak` is the viewer's section rule. Groove has no EDM label, so its frames stay out of the label loss.
  The rule's 3 songs without a drop are left out and shown separately in the report.
- `hand` reads `data/melody/labels/<id>.json`, status "done" only.
- The split comes from a hash of the song's title, about 72% train, 8% val, 20% hold-out, the same for
  both label sets, with versions of one song kept together. On the 175 songs: 128, 11, 33 plus the 3 left out.

With `weak` the labels are the rule. A high score shows the pipeline runs and the model can learn the
rule, not that it is better than the rule. Only `hand` labels can show that.

## What EDMFormer's code does

- It freezes no layers. MuQ and MusicFM are fixed only because the embeddings are computed beforehand.
- Compared with SongFormer it changes the downsampling stride from 3 to 2 (12.5 instead of 8.33 frames a
  second) and the loss weights. The weights stay compatible (strict loading passes). This pipeline keeps
  those settings.
- `prefix_dict` returns its input unchanged, so by its code the evaluation score is always 0 and upstream would
  keep step 0 as the best and stop early. `train.py` replaces `_pick_metric`.
- Its peak picking keeps only the highest boundary within 12 s either side. The rule's builds last at most 4
  bars (about 8 s), so a build's start and its drop can't both be found. `evaluate.py` scores 12 s and 4 s.
