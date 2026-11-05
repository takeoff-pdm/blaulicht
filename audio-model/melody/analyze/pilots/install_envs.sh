#!/bin/sh
# The two pilot venvs on ae-charlie. Run from /localhome/sokrates/melody:
#   sh code/analyze/pilots/install_envs.sh flamingo
#   sh code/analyze/pilots/install_envs.sh skipbart
# torch comes last on purpose, so nothing pulls a build for another CUDA afterwards.
set -e
uv=$HOME/.local/bin/uv
case "$1" in
  flamingo)
    # The card says "from our fork" but names none; upstream transformers has models/musicflamingo.
    [ -d .venv-flamingo ] || "$uv" venv --python 3.12 .venv-flamingo
    py=.venv-flamingo/bin/python
    "$uv" pip install --python $py transformers accelerate soundfile librosa scipy numpy huggingface_hub
    "$uv" pip install --python $py torch==2.11.0 torchaudio==2.11.0 --index-url https://download.pytorch.org/whl/cu128
    $py -c "import transformers, torch; print(transformers.__version__, torch.__version__, torch.version.cuda); from transformers import MusicFlamingoForConditionalGeneration as M; print(M)"
    ;;
  skipbart)
    # OpenL3 needs Keras 2 (kapre), so TensorFlow 2.15 on CPU and Python 3.11; it only embeds audio.
    # Building openl3 downloads its weights from GitHub: a 504 there fails the build, so just run this again.
    # kapre 0.3.7 asks for numpy>=2, hence 0.3.5.
    [ -d .venv-skipbart ] || "$uv" venv --python 3.11 .venv-skipbart
    py=.venv-skipbart/bin/python
    "$uv" pip install --python $py "setuptools<70" wheel "numpy==1.26.4" "tensorflow-cpu==2.15.1" "kapre==0.3.5" Cython
    "$uv" pip install --python $py "openl3==0.4.2" "transformers==4.45.2" "peft==0.13.2" soundfile librosa scipy matplotlib huggingface_hub "numpy==1.26.4"
    "$uv" pip install --python $py torch==2.11.0 torchaudio==2.11.0 --index-url https://download.pytorch.org/whl/cu128
    $py -c "import torch, transformers, peft; print(torch.__version__, torch.version.cuda, transformers.__version__, peft.__version__)"
    CUDA_VISIBLE_DEVICES= $py -c "import openl3; print('openl3', openl3.__version__)"
    ;;
  *) echo "usage: $0 flamingo|skipbart" >&2; exit 2 ;;
esac
