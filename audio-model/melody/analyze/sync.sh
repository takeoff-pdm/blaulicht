#!/bin/sh
# Push the analyzer code to ae-charlie, or pull the features it wrote (restart serve.py afterwards:
# it caches each song's analysis).
#   sh audio-model/melody/analyze/sync.sh push
#   sh audio-model/melody/analyze/sync.sh pull
set -e
here=$(cd "$(dirname "$0")" && pwd)
remote=ae-charlie:/localhome/sokrates/melody
case "$1" in
  push) rsync -a --exclude __pycache__ "$here/" "$remote/code/analyze/" ;;
  pull) mkdir -p "$here/../../data/melody/features" && rsync -a "$remote/out/features/" "$here/../../data/melody/features/" ;;
  *) echo "usage: $0 push|pull" >&2; exit 2 ;;
esac
