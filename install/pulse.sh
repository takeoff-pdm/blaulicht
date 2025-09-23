#!/usr/bin/env bash

pulseaudio --start || exit 1

pactl load-module module-native-protocol-tcp port=4656 listen=0.0.0.0 auth-anonymous=true || exit 1

exit 0

