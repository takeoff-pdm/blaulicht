#!/bin/sh
# Analyse the Rekordbox "heggemann" playlist like the first 249 songs (audio-model/melody/README.md):
# stems (htdemucs_ft on GPUs 2 + 3) and Melodia on the mix in parallel, then GAME on the vocals
# (GPU 0) and Melodia on the lead / bass stems, then the viewer data for all songs. Every step
# skips songs whose output exists; Melodia gets the full list so its index keeps every song.
cd /localhome/sokrates/melody
cp out/melodia/index.json out/melodia/index.before_hegg.json
cat songs.txt songs_hegg.txt > songs_all.txt
export CUDA_DEVICE_ORDER=PCI_BUS_ID
for s in 0 1; do
  CUDA_VISIBLE_DEVICES=$((s + 2)) /localhome/sokrates/stems/.venv/bin/python code/teacher.py --list songs_hegg_remote.txt \
    --out stems_songs --model htdemucs_ft --shard $s --num-shards 2 > logs/hegg_teacher.$s.log 2>&1 &
done
.venv-ess/bin/python code/melodia_batch.py songs_all.txt out/melodia --root audio --jobs 16 > logs/hegg_melodia.log 2>&1 &
wait
echo "$(date +%T) stems + melodia done"
CUDA_VISIBLE_DEVICES=0 .venv-game/bin/python code/game_batch.py stems_songs out/game \
  --model models/game-large/GAME-1.0.3-large-onnx > logs/hegg_game.log 2>&1 &
(cd code && ../.venv-ess/bin/python melodia_stems.py ../stems_songs ../out --jobs 12 > ../logs/hegg_stems_melodia.log 2>&1) &
wait
echo "$(date +%T) game + lead/bass done"
.venv-ess/bin/python code/viz/build_viz_data.py /localhome/sokrates/melody/viz > logs/hegg_viz.log 2>&1
echo "$(date +%T) all done"
