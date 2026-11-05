#!/bin/sh
# model analyzers for the show compiler over every song, one per GPU (drumparts on two)
cd /localhome/sokrates/melody/code
export CUDA_DEVICE_ORDER=PCI_BUS_ID
CUDA_VISIBLE_DEVICES=0 ../.venv-sep/bin/python -m analyze.run drumparts --shard 0 --num-shards 2 --force > ../logs/ana_drumparts.0.log 2>&1 &
CUDA_VISIBLE_DEVICES=3 ../.venv-sep/bin/python -m analyze.run drumparts --shard 1 --num-shards 2 --force > ../logs/ana_drumparts.1.log 2>&1 &
(CUDA_VISIBLE_DEVICES=1 ../.venv-ml/bin/python -m analyze.run events --force > ../logs/ana_events.log 2>&1; CUDA_VISIBLE_DEVICES=1 ../.venv-ml/bin/python -m analyze.run beats > ../logs/ana_beats.log 2>&1) &
wait
echo done
