#!/bin/sh
# All heads: thresholds from dev, then a split's plausibility and curve. From code/ on ae-charlie.
#   CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=3 sh analyze/fx_train/run_eval.sh calibrate
#   ... run_eval.sh dev | test
PY=../.venv-fx/bin/python
M=../models/fx_head
ALL="$M/gen_beats.pt $M/gen_both.pt $M/gen_mel.pt $M/yt_beats.pt $M/yt_both.pt $M/yt_mel.pt $M/both_beats.pt $M/both_both.pt $M/both_mel.pt $M/both_both_hn.pt"
case $1 in
  calibrate) $PY -m analyze.fx_train.evaluate calibrate $ALL ;;
  *) $PY -m analyze.fx_train.evaluate real --split $1 $ALL
     $PY -m analyze.fx_train.evaluate synth $ALL
     $PY -m analyze.fx_train.evaluate curve --split $1 $M/gen_both.pt $M/yt_both.pt $M/both_both.pt $M/both_both_hn.pt ;;
esac
echo eval-$1-done
