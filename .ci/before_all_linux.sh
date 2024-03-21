#!/bin/bash

set -e -u

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain=${RUST_VERSION:-stable} --profile=minimal -y
