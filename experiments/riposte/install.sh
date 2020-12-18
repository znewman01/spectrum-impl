#!/bin/bash
sudo apt-get update -y > /dev/null
sudo apt-get install -y golang-go libssl-dev
go get golang.org/x/crypto/nacl/box

sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf

go get bitbucket.org/henrycg/riposte
RIPOSTE_BASE=go/src/bitbucket.org/henrycg/riposte
cd "$RIPOSTE_BASE"
git checkout multiparty
# we'll need to rebuild each time since the parameters are hardcoded but we
# still build here to pre-fetch dependencies
go build ./...  
