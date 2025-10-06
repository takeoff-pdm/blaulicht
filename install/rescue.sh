#!/usr/bin/env bash

IP=$(ip route get 1.1.1.1 | grep -oP 'src \K\S+')
UPTIME=$(uptime)
DISK=$(df -h | grep /dev/sda1)
TEMPERATURE=$(sensors | grep -E 'Core|temp1' | head -n1 | awk '{print $2}')

echo "============== BLAULICHT RESCUE =============="
echo "    - IP : ${IP}"
echo "    - UP : ${UPTIME}"
echo "    - DSK: ${DISK}"
echo "    - TMP: ${TEMPERATURE}"
echo "=============================================="
