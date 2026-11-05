#!/bin/bash
# Data prep chain. Waits for each dependency, logs to pipeline.log.
cd /localhome/sokrates/stems
log() { echo "$(date +%H:%M:%S) $*"; }
until grep -q MUSDB_DONE data/musdb.log; do sleep 30; done
log "musdb unzipped"
.venv/bin/python code/teacher.py --musdb data/musdb18hq/train --out data/teacher/musdb_train > logs_musdb_convert.txt 2>&1
log "musdb train converted: $(ls data/teacher/musdb_train/*.npy | wc -l) tracks"
until grep -q FMA_DL_DONE data/fma.log; do sleep 30; done
unzip -q data/fma_medium.zip -d data/ && rm data/fma_medium.zip
find data/fma_medium -name "*.mp3" | shuf --random-source=<(yes) | head -6000 > data/fma_files.txt
log "fma unzipped, $(wc -l < data/fma_files.txt) clips selected"
while pgrep -f "teacher.py --list data/edm_files" > /dev/null; do sleep 30; done
log "edm teacher done: $(ls data/teacher/edm/*.npy | wc -l)"
for s in 0 1; do
  CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=3 .venv/bin/python code/teacher.py --list data/fma_files.txt \
    --out data/teacher/fma --model htdemucs --shard $s --num-shards 2 > logs_teacher_fma.$s.txt 2>&1 &
done
wait
log "fma teacher done: $(ls data/teacher/fma/*.npy | wc -l)"
