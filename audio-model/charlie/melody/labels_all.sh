#!/bin/sh
cd /localhome/sokrates/melody
P=.venv-ess/bin/python
T=/localhome/sokrates/stems/data/teacher
$P code/melodia_labels.py --list songs_remote.txt --audio-out data/audio/songs --out data/labels/songs
$P code/melodia_labels.py --list musdb_test_mix.txt --audio-out data/audio/musdb_test --out data/labels/musdb_test
$P code/melodia_labels.py --npy-dir $T/edm --out data/labels/edm
$P code/melodia_labels.py --npy-dir $T/musdb_train --out data/labels/musdb_train
$P code/melodia_labels.py --npy-dir $T/fma --out data/labels/fma
