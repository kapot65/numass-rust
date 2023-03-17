#!/bin/bash
sudo ip addr add 10.0.0.4/24 dev enp1s0d1
sudo ip route add 10.0.0.5 dev enp1s0d1
sudo ip route add 10.0.0.6 dev enp1s0d1