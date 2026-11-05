#!/bin/sh
# lyrics as the vocals analyzer writes separated vocals; stops when it is done and all are transcribed
cd /localhome/sokrates/melody/code
NV=../.venv-asr/lib/python3.12/site-packages/nvidia
export LD_LIBRARY_PATH=$NV/cublas/lib:$NV/cudnn/lib:$LD_LIBRARY_PATH CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=3
while :; do
  ../.venv-asr/bin/python -m analyze.run lyrics 2>&1 | grep -v "no separated vocals" | tail -n 2
  pgrep -f "[a]nalyze.run vocals" > /dev/null || break
  sleep 120
done
../.venv-asr/bin/python -m analyze.run lyrics 2>&1 | tail -n 2
echo done
