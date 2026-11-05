#!/bin/sh
# usage: run_game_follow.sh <shard> <num_shards> <gpu>
# Transcribes vocals as the teacher writes stems; stops once the teacher is done and all are transcribed.
cd /localhome/sokrates/melody
while :; do
  CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$3 .venv-game/bin/python code/game_batch.py stems_songs out/game \
    --model models/game-large/GAME-1.0.3-large-onnx --shard $1 --num-shards $2 2>&1 | grep -v -E '\[W:|Memcpy'
  pgrep -f "code/teacher.py --list songs_remote.txt" > /dev/null || break
  sleep 60
done
CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$3 .venv-game/bin/python code/game_batch.py stems_songs out/game \
  --model models/game-large/GAME-1.0.3-large-onnx --shard $1 --num-shards $2 2>&1 | grep -v -E '\[W:|Memcpy'
echo done
