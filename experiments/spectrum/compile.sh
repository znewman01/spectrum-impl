#!/bin/bash
# Ensures that the VM has an up-to-date copy of the Spectrum binaries.
#
# By up-to-date, I mean with Git SHA $SRC_SHA.
# Compiled binaries cached in S3 in case we want to change system settings
# without recompiling (which is super slow).
# Binaries go to $HOME/ubuntu/spectrum/.
#
# Need Spectrum source in $HOME/spectrum-src.tar.gz and AWS credentials set via environment variable.
set -x
set -eufo pipefail

S3_BUCKET=hornet-spectrum
ARCHIVE_NAME="spectrum-${SRC_SHA}-${INSTANCE_TYPE}-${PROFILE}"
S3_OBJECT="s3://${S3_BUCKET}/${ARCHIVE_NAME}"

object_exists=$(aws s3api head-object --bucket $S3_BUCKET --key $ARCHIVE_NAME || true)
if [[ -z "$object_exists" ]]; then
    tar -xzf spectrum-src.tar.gz
    cd $HOME/spectrum

    if [ "${PROFILE}" = "release" ]; then
        RELEASE_FLAG="--release"
    else
        RELEASE_FLAG=""
    fi
    $HOME/.cargo/bin/cargo build --bins $RELEASE_FLAG

    cd $HOME/spectrum/target
    tar -czf $HOME/spectrum-bin.tar.gz \
        --transform "s/${PROFILE}/spectrum/" \
        "${PROFILE}"/{publisher,worker,leader,viewer,broadcaster,setup} \
        ../spectrum/data/{server,ca}.{crt,key}

    cd $HOME
    rm -rf spectrum/  # save 3GB, at least in debug mode

    aws s3 cp "$HOME/spectrum-bin.tar.gz" "$S3_OBJECT"
else
    aws s3 cp "$S3_OBJECT" "$HOME/spectrum-bin.tar.gz"
fi

cd $HOME
tar -xzf spectrum-bin.tar.gz
