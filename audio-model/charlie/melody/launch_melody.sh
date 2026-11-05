#!/bin/sh
# usage: launch_melody.sh <gpu> <run name> <args...>
gpu=$1; name=$2; shift 2
cd /localhome/sokrates/melody/code
mkdir -p ../runs/$name
CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=$gpu nohup ../.venv-train/bin/python train_melody.py --out ../runs/$name "$@" > ../runs/$name/train.log 2>&1 &
echo started $name $!
