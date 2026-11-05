#!/bin/bash
cd /localhome/sokrates/stems
done_run() { grep -q "\"step\": 40000" runs/$1.log; }
evalrun() {  # gpu run...
  gpu=$1; shift
  for r in "$@"; do
    until done_run $r; do sleep 60; done
    sleep 30
    CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$gpu .venv/bin/python code/evaluate.py \
      --ckpt runs/$r/last.pt --out runs/eval_$r.json > runs/eval_$r.log 2>&1
    echo "$(date +%H:%M:%S) evaluated $r"
  done
}
evalrun 0 gt_la30 la100 nomusdb_la30 &
evalrun 3 la0 la10 la30 &
wait
echo "$(date +%H:%M:%S) ALL_EVALS_DONE"
