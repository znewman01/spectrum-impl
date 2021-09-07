#!/bin/bash
set -x
set -eufo pipefail

sudo apt-get update -y > /dev/null
sudo apt-get install -y \
     build-essential \
     libssl-dev \
     pkg-config \
     unzip \
     m4 \
     etcd \
     iperf

curl https://sh.rustup.rs -sSf | sh -s -- \
    -y \
    --default-toolchain nightly-2021-08-26  # TODO

curl https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip \
    -o "awscliv2.zip"
unzip -q awscliv2.zip
sudo ./aws/install

sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf

sudo mv "${HOME}/config/publisher.service" "/etc/systemd/system/spectrum-publisher.service"
sudo mv "${HOME}/config/leader.service" "/etc/systemd/system/spectrum-leader.service"
sudo mv "${HOME}/config/worker@.service" "/etc/systemd/system/spectrum-worker@.service"
sudo mv "${HOME}/config/viewer@.service" "/etc/systemd/system/viewer@.service"
sudo mv "${HOME}/config/broadcaster@.service" "/etc/systemd/system/broadcaster@.service"
sudo mkdir -p "/etc/systemd/system/etcd2.service.d/"
sudo mv "${HOME}/config/etcd.conf" "/etc/systemd/system/etcd2.service.d/args.conf"
