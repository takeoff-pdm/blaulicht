#!/bin/sh
# usage: run_gpu.sh <gpu> <logfile> <python args...>
gpu=$1; log=$2; shift 2
cd /localhome/sokrates/stems
CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$gpu HF_HUB_DISABLE_TELEMETRY=1 nohup .venv/bin/python "$@" >> "$log" 2>&1 &
echo started $!
