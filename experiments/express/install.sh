#!/bin/bash
sudo apt-get update -y > /dev/null
sudo apt-get install -y golang-go libssl-dev
go get golang.org/x/crypto/nacl/box


sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf

# TODO: pin to a specific commit
git clone https://github.com/SabaEskandarian/Express.git
EXPRESS_ROOT="$HOME/Express/"
for buildDir in client serverA serverB; do
    cd "${EXPRESS_ROOT}/${buildDir}"
    go build
done
