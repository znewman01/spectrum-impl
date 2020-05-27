#!/bin/bash
sudo apt-get update -y > /dev/null
sudo apt-get install -y golang-go libssl-dev
go get golang.org/x/crypto/nacl/box

git clone https://github.com/SabaEskandarian/Express.git
EXPRESS_ROOT="$HOME/Express/"
for buildDir in client serverA serverB; do
    cd "${EXPRESS_ROOT}/${buildDir}"
    go build
done
