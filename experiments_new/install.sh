#!/bin/bash -e
set -x

sudo apt update -y
sudo apt install -y \
     build-essential \
     libssl-dev \
     pkg-config \
     unzip \
     m4

curl https://sh.rustup.rs -sSf | sh -s -- \
    -y \
    --default-toolchain nightly-2020-03-22

curl https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip \
    -o "awscliv2.zip"
unzip awscliv2.zip
sudo ./aws/install
