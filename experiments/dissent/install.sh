#!/bin/bash
set -euf

sudo apt-get update -y > /dev/null
sudo apt-get install -y qtchooser qt5-default g++ make unzip

wget https://www.cryptopp.com/cryptopp562.zip
unzip -d cryptopp cryptopp562.zip
pushd cryptopp
sed -i 's/# \(CXXFLAGS += -fPIC\)/\1/' GNUmakefile
sed -i 's/static int tt\[10\]/static unsigned int tt\[10\]/g' wake.cpp
make
make libcryptopp.so
sudo make install
popd

git clone https://github.com/dedis/Dissent
pushd Dissent
git checkout 84c79e038d4137004244ca41f2d06726c67dc632
# C++ changes since 2014 added some additional warnings
sed -i 's/-Werror/-Wall/g' dissent.pro
for project in dissent.pro keygen.pro application.pro; do
    qmake "DEFINES += DEMO_SESSION" "DEFINES += BAD_CS_BULK" $project
    make
done
popd
# TODO: make bigger
./Dissent/keygen --nkeys=10000

sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf
