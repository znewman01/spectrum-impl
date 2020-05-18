#!/bin/bash
set -x
set -eufo pipefail

sudo apt-get update -y
sudo apt-get install -y \
     build-essential \
     libssl-dev \
     pkg-config \
     unzip \
     m4 \
     etcd \
     nginx

curl https://sh.rustup.rs -sSf | sh -s -- \
    -y \
    --default-toolchain nightly-2020-03-22

curl https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip \
    -o "awscliv2.zip"
unzip -q awscliv2.zip
sudo ./aws/install

sudo cp "$HOME/config/nginx.conf" /etc/nginx/nginx.conf
sudo cp "$HOME/config/sysctl.conf" /etc/sysctl.d/20-spectrum.conf

# Nginx pass-throughs
# This will probably not work when we want to run on multiple servers
externals=(5000 5001 5100 5101 5102 5103)
internals=(6000 6001 6100 6101 6102 6103)
for (( i=0; i<${#internals[*]}; ++i)); do
    export internal=${internals[$i]}
    export external=${externals[$i]}
    envsubst '$internal $external' < "${HOME}/config/nginx.conf.template" \
        | sudo tee "/etc/nginx/conf.d/proxy-${external}-${internal}.conf" > /dev/null
done

sudo mv "${HOME}/config/publisher.service" "/etc/systemd/system/spectrum-publisher.service"
sudo mv "${HOME}/config/leader.service" "/etc/systemd/system/spectrum-leader.service"
sudo mv "${HOME}/config/worker@.service" "/etc/systemd/system/spectrum-worker@.service"
sudo mv "${HOME}/config/viewer@.service" "/etc/systemd/system/viewer@.service"
sudo mv "${HOME}/config/broadcaster@.service" "/etc/systemd/system/broadcaster@.service"
