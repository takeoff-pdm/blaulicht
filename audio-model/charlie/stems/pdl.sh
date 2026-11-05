#!/bin/bash
# usage: pdl.sh <url> <file> <total_bytes> <parts> <done_marker_log> <post-cmd>
url=$1; f=$2; total=$3; n=$4; marker=$5; post=$6
have=$(stat -c %s "$f"); rest=$((total-have)); chunk=$(( (rest+n-1)/n ))
for i in $(seq 0 $((n-1))); do
  a=$((have+i*chunk)); b=$((a+chunk-1)); [ $b -ge $total ] && b=$((total-1))
  ( until curl -sL --fail -r $a-$b -o "$f.part$i" "$url" && [ $(stat -c %s "$f.part$i") -eq $((b-a+1)) ]; do sleep 5; done ) &
done
wait
for i in $(seq 0 $((n-1))); do cat "$f.part$i" >> "$f" && rm "$f.part$i"; done
echo "size $(stat -c %s "$f") expected $total" >> "$marker"
eval "$post" >> "$marker" 2>&1
