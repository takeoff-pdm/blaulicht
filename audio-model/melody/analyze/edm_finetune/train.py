"""Fine-tune SongFormer on the EDM labels, through EDMFormer's own training loop. Runs on ae-charlie.

    CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0 ../.venv-sf/bin/python -m analyze.edm_finetune.train \
        --labels weak --run dryrun --max-steps 1000 --eval-every 200

Writes to models/edm_finetune/<labels>_<run>/: config.yaml, val_metrics.jsonl, model.ckpt-<step>.pt
(the best one on the validation songs and the last are kept) and best.txt. Starting it again resumes.
The hold-out songs are never touched here; `evaluate.py` is the only thing that reads them.

What is trained: everything after the embeddings (their linear fusion, the downsampling, the dataset
embedding, the transformer, both heads). MuQ and MusicFM stay fixed because the embeddings are computed
beforehand. EDMFormer's code freezes nothing else, its paper's architecture figure says the same.

Worked around from here rather than in the clone:
- `prefix_dict` returns its input unchanged when given a prefix, so `_pick_metric` never finds
  "valid_HitRate_..." and every evaluation scores 0: upstream always keeps step 0 as the best and stops
  after `early_stopping_step` evaluations. `_pick_metric` is replaced.
- `_target_` names a dataset module that isn't in the repo (edmformer_gcs_dataset); ours wraps the local one.
- Upstream's infer.py reads SongFormer.safetensors as EMA weights, and `--pretrained_ckpt` without strict
  loading would silently miss EMA-style names ("ema_model.*"); they are converted and loaded strictly.
- The dataset embedding of id 9 (EDMFormer) was never trained, so it starts as a copy of id 5, the
  one SongFormer's inference uses.
- EMA set to update_every 1, beta 0.999: the library's defaults are meant for long runs and would lag
  behind on a short one (from memory of ema_pytorch's defaults, not measured here).
"""

import argparse
import json
import os
import re
import sys
from argparse import Namespace
from pathlib import Path

from analyze import common

HERE = Path(__file__).resolve().parent
SRC = common.ROOT / "models/src/EDMFormer/src/SongFormer"
BASE_WEIGHTS = common.ROOT / "models/songformer_hf/SongFormer.safetensors"
EMBEDDINGS = common.ROOT / "out/edm_embeddings"
SETS = ("musicfm_30s", "muq_30s", "musicfm_420s", "muq_420s")
DATASET_ID, STOCK_DATASET_ID = 9, 5


def add_upstream_to_path():
    import numpy as np
    import scipy

    scipy.inf = np.inf  # msaf, which the model imports for its metrics, still wants it
    for p in (str(SRC / "train"), str(SRC)):
        if p not in sys.path:
            sys.path.insert(0, p)


def base_checkpoint(out: Path) -> Path:
    """The stock SongFormer weights as a plain state dict, with the EDM dataset embedding seeded."""
    import torch
    from safetensors.torch import load_file

    path = out / "base.pt"
    if not path.exists():
        raw = load_file(str(BASE_WEIGHTS))
        state = {k.removeprefix("ema_model."): v for k, v in raw.items() if k.startswith("ema_model.")} or raw
        emb = state["dataset_class_prefix.weight"].clone()
        emb[DATASET_ID] = emb[STOCK_DATASET_ID]
        state["dataset_class_prefix.weight"] = emb
        torch.save({"model": state}, path)
    return path


def make_config(args, out: Path):
    from omegaconf import OmegaConf

    hp = OmegaConf.load(SRC / "configs/SongFormer.yaml")
    data = args.data or HERE / "data" / args.labels
    dirs = " ".join(str(EMBEDDINGS / name) for name in SETS)
    for key, split in (("train_dataset", "train"), ("eval_dataset", "val")):
        hp[key]._target_ = "analyze.edm_finetune.edm_dataset.Dataset"
        hp[key].dataset_abstracts = [{"internal_tmp_id": f"EDM-{split}", "dataset_type": "EDMFormer", "input_embedding_dir": dirs,
                                      "label_path": str(data / "labels.jsonl"), "split_ids_path": str(data / f"{split}.txt"), "multiplier": 1}]
    hp.warmup_steps = args.warmup
    hp.total_steps = args.max_steps
    hp.warmup_max_lr = args.lr
    hp.early_stopping_step = args.patience
    hp.ema_kwargs = {"update_after_step": min(200, args.warmup), "update_every": 1, "beta": 0.999}
    hp.train_dataloader.num_workers = args.workers
    # the loader drops incomplete batches: fewer songs than a batch would loop over nothing, forever
    n_train = len((data / "train.txt").read_text().split())
    hp.train_dataloader.batch_size = max(1, min(args.batch, n_train))
    OmegaConf.save(hp, out / "config.yaml")
    return hp


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", required=True, help="dataset under data/")
    ap.add_argument("--data", type=Path, help="the dataset's folder, if it isn't under data/")
    ap.add_argument("--run", required=True)
    ap.add_argument("--max-steps", type=int, default=1000, help="optimizer steps; two batches each")
    ap.add_argument("--eval-every", type=int, default=200)
    ap.add_argument("--warmup", type=int, default=100)
    ap.add_argument("--lr", type=float, default=1e-4)
    ap.add_argument("--patience", type=int, default=4, help="validation evaluations without improvement before stopping")
    ap.add_argument("--batch", type=int, default=4)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--out", type=Path, help="default models/edm_finetune/<labels>_<run>")
    args = ap.parse_args()

    out = args.out or common.ROOT / "models/edm_finetune" / f"{args.labels}_{args.run}"
    out.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault("OMP_NUM_THREADS", "4")
    add_upstream_to_path()
    hp = make_config(args, out)
    base = base_checkpoint(out)

    import train as upstream  # EDMFormer's train/train.py; `python -m analyze...` keeps this file out of its way

    metrics_file = out / "val_metrics.jsonl"
    original_evaluate = upstream.evaluate

    def evaluate(model, eval_data_loader, accelerator, global_step):
        result = original_evaluate(model, eval_data_loader, accelerator, global_step)
        row = {"step": global_step, **{k: float(v) for k, v in result.items() if k.startswith("overall_")}}
        with metrics_file.open("a") as f:
            f.write(json.dumps(row) + "\n")
        print("val", json.dumps({k: round(v, 3) for k, v in row.items() if k in ("step", "overall_acc", "overall_HitRate_0.5F", "overall_HitRate_3F", "overall_loss")}), flush=True)
        return result

    def pick_metric(eval_res, dataset_id, metric_name):
        for key in (f"dataset_{dataset_id}_{metric_name}", f"overall_{metric_name}"):
            if key in eval_res:
                return eval_res[key], key
        return None, None

    upstream.evaluate = evaluate
    upstream._pick_metric = pick_metric

    # One step more than asked: the loop stops before it evaluates and saves at max_steps itself.
    run_args = Namespace(init_seed=args.seed, run_name=out.name, model_name="SongFormer", pretrained_ckpt=str(base), pretrained_key="model",
                         pretrained_strict=True, checkpoint_dir=str(out), tags=None, max_epochs=100000, max_steps=args.max_steps + 1,
                         save_interval=args.eval_every, eval_interval=args.eval_every, log_interval=20, keep_step=True)
    upstream.run_training(run_args, hp)
    finish(out)


def finish(out: Path) -> None:
    """Pick the best step on the validation songs (label accuracy + mean boundary hit rate), remove the others."""
    rows = [json.loads(line) for line in (out / "val_metrics.jsonl").read_text().splitlines()]
    score = lambda r: r["overall_acc"] + 0.5 * (r["overall_HitRate_0.5F"] + r["overall_HitRate_3F"])
    best = max((r for r in rows if r["step"] > 0), key=score, default=rows[-1])
    (out / "best.txt").write_text(f"model.ckpt-{best['step']}.pt\n")
    print("best on validation:", json.dumps(best), flush=True)
    keep = {f"model.ckpt-{best['step']}.pt", (out / "checkpoint").read_text().strip()}
    for path in out.glob("model.ckpt-*.pt"):
        if re.fullmatch(r"model\.ckpt-\d+\.pt", path.name) and path.name not in keep:
            path.unlink()


if __name__ == "__main__":
    main()
