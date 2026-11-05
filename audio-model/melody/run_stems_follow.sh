#!/bin/sh
# Melodia on the lead and bass stems as the teacher writes them; stops when the teacher is done.
cd /localhome/sokrates/melody/code
while :; do
  ../.venv-ess/bin/python melodia_stems.py ../stems_songs ../out --jobs 8 2>&1 | grep -v INFO
  pgrep -f "code/teacher.py --list songs_remote.txt" > /dev/null || break
  sleep 120
done
../.venv-ess/bin/python melodia_stems.py ../stems_songs ../out --jobs 8 2>&1 | grep -v INFO
echo done
