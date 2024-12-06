#!/usr/bin/env bash

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)
BASE_DIR=$(realpath "$SCRIPT_DIR/../")

# Number of iterations
NUM_ITERATIONS=${1:-"1"}

mkdir -p "${BASE_DIR}/etc/data/nodekeys"

# Array to store public keys
public_keys=()

# Loop for the specified number of iterations
for ((i=0; i<NUM_ITERATIONS; i++)); do
  echo "Iteration $i"
  # Call the binary to create a key
  bootnode -genkey "${BASE_DIR}/etc/data/nodekeys/nodekey_$i"
  # Call the binary to get the public key and append to the array
  public_key=$(bootnode -writeaddress -nodekey "${BASE_DIR}/etc/data/nodekeys/nodekey_$i")
  public_keys+=("$i: $public_key")
done

# Return the array with indices
echo "Public keys with indices:"
for ((i=0; i<NUM_ITERATIONS; i++)); do
  echo "nodekey_$i: ${public_keys[i]}"
done