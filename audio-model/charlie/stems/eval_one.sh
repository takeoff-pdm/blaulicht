#!/bin/bash
# usage: eval_one.sh <gpu> <run>...   evaluates each run once it has finished
cd /localhome/sokrates/stems
gpu=$1; shift
for r in "$@"; do
  until grep -q "\"step\": 40000" runs/$r.log; do sleep 60; done
  while ps -eo args | grep -q "[c]ode/train.py --out runs/$r "; do sleep 10; done
  CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$gpu .venv/bin/python code/evaluate.py \
    --ckpt runs/$r/last.pt --out runs/eval_$r.json > runs/eval_$r.log 2>&1
  echo "$(date +%H:%M:%S) evaluated $r" >> final_eval.log
done
