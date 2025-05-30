#!/usr/bin/env bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/regtest_pegin.sh

address_arr=("0xc08E0773F0cDe0311F97f8c0CF05528c1D0b025C" "0x56674562b448cB3be2C82771F13d64da32726908" "0xF7cA0A228a691AD222Be4F17913f5d91Fc5d1F72" "0x2E88B74d9c519F2aeC9b0228C8a1B3Da086613f3" "0x2Be0fC344B26bADeb71489eB86EaB20c6818d0bf" "0x14093bf1F0f4f24C99eA62700315B4A5b8673Bcc" "0x8f10eD34156ED0F2d887431720716e1f33d60E6B" "0x841A3EAEa1b22bb688eeddb8a44E24a110b93d32" "0x313ab894e7ecb38de35f192689219ed341b4E11a" "0xe25e717Cfe659d09B890389e04DEa133c2CF473C")

for element in "${address_arr[@]}"; do
  ./scripts/regtest_pegin.sh "50.0" $element
done