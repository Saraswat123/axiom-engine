#!/usr/bin/env bash
# Run axiom-engine with RISC Zero ZK proofs enabled
# Requires: rzup install (run once)

set -e

export PATH="$HOME/.risc0/bin:$PATH"

# Dev mode = instant mock proofs (no hardware required)
# Set RISC0_DEV_MODE=0 for real STARK proofs (slow locally — use Bonsai instead)
export RISC0_DEV_MODE=${RISC0_DEV_MODE:-1}
export AXIOM_TRANSPORT=http
export PORT=8080

echo "RISC0_DEV_MODE=$RISC0_DEV_MODE"
echo "Starting axiom-engine with ZK proofs..."

cargo run --release -p axiom-mcp-server --features axiom-zk-proof/risc0
