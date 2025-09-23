#!/bin/bash
# Disable screensaver and power management
xset s off
xset -dpms
xset s noblank

# TODO: do something to relays?

# Launch blaulicht
blaulicht

xterm
