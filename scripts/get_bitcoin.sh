#!/usr/bin/env bash

set -e

VERSION=27.0

# For seamless cross-platform support. If OS is not supported, exit with an error message.
if [[ "$OSTYPE" == "darwin"* ]]; then
    TARGET_ARCH=arm64-apple-darwin
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    TARGET_ARCH=x86_64-linux-gnu
else
    echo "This installation script does NOT support your OS. Please refer to the official Bitcoin Core documentation for installation instructions."
    exit 1
fi

BTC_RELEASE=${VERSION}-${TARGET_ARCH}
curl -O "https://bitcoincore.org/bin/bitcoin-core-${VERSION}/bitcoin-${BTC_RELEASE}.tar.gz"

tar xzvf bitcoin-${BTC_RELEASE}.tar.gz
cp bitcoin-${VERSION}/bin/{bitcoind,bitcoin-cli} ./

# To support users on macOS, we need to codesign the binaries to avoid the "cannot be opened because the developer cannot be verified" error
if [[ "$TARGET_ARCH" == "arm64-apple-darwin" ]]; then
    chmod +x bitcoin-cli
    chmod +x bitcoind

    codesign -s - bitcoin-cli 
    codesign -s - bitcoind
fi

# Cleanup the downloaded files and extracted directory
rm -rf *.tar.gz
rm -rf ./bitcoin-${VERSION}