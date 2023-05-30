#!/bin/bash
# ETH_NAME="enp1s0d1"
# ETH_NAME="enp0s31f6"
ETH_NAME="enp1s0"
sudo ip addr add 10.0.0.4/24 dev ${ETH_NAME}
sudo ip route add 10.0.0.5 dev ${ETH_NAME}
sudo ip route add 10.0.0.6 dev ${ETH_NAME}