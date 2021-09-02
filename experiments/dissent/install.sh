#!/bin/bash
git clone https://github.com/dedis/Dissent

sudo apt-get update -y > /dev/null
sudo apt-get install -y qtchooser qt5-default g++ make unzip

wget https://www.cryptopp.com/cryptopp562.zip
unzip -d cryptopp cryptopp562.zip
pushd cryptopp
sed -i 's/# \(CXXFLAGS += -fPIC\)/\1/' GNUmakefile
sed -i 's/static int tt\[10\]/static unsigned tt\[10\]/g' wake.cpp
make
make libcryptopp.so
sudo make install
popd

pushd Dissent
git checkout 84c79e038d4137004244ca41f2d06726c67dc632
sed -i 's/-Werror/-Wall/g' dissent.pro
qmake dissent.pro
make
qmake keygen.pro
make
popd

sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf
