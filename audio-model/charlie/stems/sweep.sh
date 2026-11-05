#!/bin/bash
cd /localhome/sokrates/stems


echo "$(date +%H:%M:%S) fma clips: $(ls data/teacher/fma/*.npy | wc -l)"
common="--steps 40000 --workers 7 --eval-every 5000"
./run_gpu.sh 3 runs/la0.log code/train.py --out runs/la0 --lookahead 0 $common
./run_gpu.sh 3 runs/la10.log code/train.py --out runs/la10 --lookahead 10 $common
./run_gpu.sh 3 runs/la30.log code/train.py --out runs/la30 --lookahead 30 $common
./run_gpu.sh 0 runs/la100.log code/train.py --out runs/la100 --lookahead 100 $common
./run_gpu.sh 0 runs/nomusdb_la30.log code/train.py --out runs/nomusdb_la30 --lookahead 30 --musdb 0 $common
echo "$(date +%H:%M:%S) sweep launched"
