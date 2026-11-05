#!/bin/sh
# DSP analyzers for the show compiler, over every song with stems
cd /localhome/sokrates/melody/code
for a in stereo timbre risers; do ../.venv-ana/bin/python -m analyze.run $a --jobs 16 > ../logs/ana_$a.log 2>&1; done
echo done
