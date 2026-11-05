#!/bin/sh
# The heads of the comparison, from code/ on ae-charlie, once the mixtures are cached. Names are
# <source>_<features>: source gen, yt or both, features BEATs, log-mel or both.
#   CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=3 sh analyze/fx_train/run_all.sh gen yt both
PY=../.venv-fx/bin/python
LOGS=../logs/fx
for need in "$@"; do
  [ "$need" = both ] && continue
  until [ -f ../tmp/fx/mix/train_$need.json ] && [ -f ../tmp/fx/mix/val_$need.json ]; do sleep 30; done
done
for features in beats both mel; do
  pids=""
  for source in "$@"; do
    case $source in both) src=gen,yt ;; *) src=$source ;; esac
    $PY -m analyze.fx_train.train --name ${source}_$features --sources $src --features $features > $LOGS/train_${source}_$features.log 2>&1 &
    pids="$pids $!"
  done
  wait $pids
done
echo all heads done
