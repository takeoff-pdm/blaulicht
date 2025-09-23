#!/bin/bash
# Disable screensaver and power management
xset s off
xset -dpms
xset s noblank

# TODO: do something to relays?
#

xrandr --newmode "1024x600_60.00"   49.00  1024 1064 1168 1312  600 603 613 624 -hsync +vsync
xrandr --addmode VGA-1 "1024x600_60.00"
xrandr --output VGA-1 --mode "1024x600_60.00"

# Launch blaulicht
blaulicht

xterm
