#!/bin/sh
cd /localhome/sokrates/melody/code
export CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0
while pgrep -f "[a]nalyze.run songformer" > /dev/null; do sleep 5; done
../.venv-sf/bin/python -m analyze.run songformer > ../logs/sf_all3.log 2>&1
../.venv-sf/bin/python -m analyze.edm_finetune.train --labels weak --run dryrun --max-steps 1000 --eval-every 200 > ../logs/sf_train_dryrun.log 2>&1
../.venv-sf/bin/python -m analyze.edm_finetune.evaluate --labels weak --run dryrun --checkpoint best > ../logs/sf_eval_best.log 2>&1
../.venv-sf/bin/python -m analyze.edm_finetune.evaluate --labels weak --run dryrun --checkpoint base --out ../out/edm_finetune/weak_dryrun_base > ../logs/sf_eval_base.log 2>&1
echo chain-finished > ../logs/sf_chain.done
