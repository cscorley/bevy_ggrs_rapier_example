#!/bin/bash

if tc qdisc show | grep "netem"; then
    tc qdisc del dev lo root handle 1:
else
    tc qdisc add dev lo root handle 1: htb default 12
    tc class add dev lo parent 1:1 classid 1:12 htb rate 2000kbps ceil 2000kbps
    tc qdisc add dev lo parent 1:12 netem delay 100ms
fi

ping -c 3 localhost
#ping -c 3 google.com
