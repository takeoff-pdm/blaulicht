# Files that only existed on ae-charlie

Small files from the training host that can't be recreated, copied on 2026-11-05 so they don't depend on that machine. Paths inside the scripts are as on ae-charlie.

- `envs/<name>.txt`: the exact packages of each environment, read from what's installed, with the Python version on the first line. `<name>` is `/localhome/sokrates/melody/.venv-<name>`; `stems` is `/localhome/sokrates/stems/.venv`. Torch builds keep their suffix (`+cu128`), so install those from PyTorch's matching index.
- `clones.txt`: the third-party repositories and the commit each is on. None has code changes.
- `melody/`: run scripts from the melody folder on ae-charlie, and `teacher.py`, which `run_hegg.sh` calls. `melody/songformer/`: the install list that `audio-model/melody/analyze/edm_finetune/README.md` cites, plus its download and run scripts.
- `stems/`: the stem project's pipeline scripts.

The effect head's weights are in `audio-model/runs/fx_head/` on the Mac: gitignored, so a second copy but not in the repo. `best.pt` goes back to `models/fx_head/` on ae-charlie.

## Left on ae-charlie on purpose

These can be rebuilt, but nothing else holds a copy.

| What | Size | To rebuild |
|---|---|---|
| Separated stems (`melody/stems_songs`, `melody/stems_vocals`) and `melody/data` | 41 GB | GPU hours and the songs; `melody/run_hegg.sh` here shows the steps |
| YouTube effect sounds (`melody/models/fx_head/sources`) | 623 MB | `audio-model/melody/analyze/fx_train/yt_manifest.json`, if the videos still exist |
| SongFormer dry run (`melody/models/edm_finetune`, `melody/out/edm_embeddings`) | 18.5 GB | About an hour, see `audio-model/melody/analyze/edm_finetune/README.md` |
| Stem project data and runs (`stems/data`, `stems/runs`) | 130 GB | `stems/pipeline.sh` here; the Mac has some of the runs in `audio-model/stems/runs` |
| Downloaded model weights and clones (`melody/models`) | about 30 GB | Download again; `clones.txt` has the commits |
| The environments themselves | over 115 GB | Reinstall from `envs/` |
