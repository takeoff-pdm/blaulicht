#!/usr/bin/env bash

scp ./dist/blaulicht-x86_64-unknown-linux-gnu.tar.gz bl:~/
ssh root@bl '/home/blaulicht/update.sh'
