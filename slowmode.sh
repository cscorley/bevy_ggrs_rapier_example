#!/usr/bin/env bash

if sudo tc qdisc show | grep "netem"; then
    sudo tc qdisc del dev lo root handle 1:
else
    sudo tc qdisc add dev lo root handle 1: htb default 12
    sudo tc class add dev lo parent 1:1 classid 1:12 htb rate 2000kbps ceil 2000kbps
    sudo tc qdisc add dev lo parent 1:12 netem delay 100ms
fi

ping -c 3 localhost
#ping -c 3 google.com
