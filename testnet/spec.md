# Axiom Engine — Testnet Specification

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Network ID | 42161337 (axiom-devnet) |
| Consensus | PoA (dev mode) → BLS PoS (testnet) |
| Block time | 2s |
| Gas limit | 30,000,000 |
| Native token | AXM (18 decimals) |

## Testnet Phases

### Phase T1 — Local devnet (current)
- Single node, Geth --dev
- axiom-engine HTTP on localhost:8080
- Z3 + egg + compute tools live
- No persistence

### Phase T2 — 3-node local cluster
```bash
# Start 3 nodes with docker-compose
docker-compose --profile cluster up
# Nodes: localhost:8080, 8081, 8082
# Gossip: libp2p over localhost
```

### Phase T3 — Public testnet
- 10 validator nodes (binary scale initial set)
- Espresso shared sequencer integration
- L2 rollup: Arbitrum Orbit (settles to axiom L1)
- Explorer: axiom-testnet.explorer (TBD)

## Smart Contracts (Phase T2)

### ProofAnchor.sol
```solidity
// Anchors ZK proof receipts on-chain
// axiom-engine posts receipt hash after every proven computation
interface IProofAnchor {
    function anchor(bytes32 receiptHash, bytes32 imageId) external;
    function verify(bytes32 receiptHash) external view returns (bool);
}
```

### InvariantRegistry.sol
```solidity
// On-chain mirror of .agents/invariants.yml
// Z3 verifies off-chain → stores verdict on-chain
interface IInvariantRegistry {
    function recordVerdict(string calldata invariantId, bool proved) external;
    function getVerdict(string calldata invariantId) external view returns (bool, uint256);
}
```

## Local Test Setup

```bash
# 1. Start local testnet
docker-compose up axiom-testnet axiom-engine

# 2. Verify engine health
curl http://localhost:8080/health

# 3. Call a tool
curl -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"opt_verify","input":{"expression":"(+ x 0)"}}'

# 4. Run full test suite
cargo test --all

# 5. Run integration tests against local testnet
cargo test --test integration -- --test-threads=1
```

## Data Availability (Phase T3)

Proof receipts stored as EIP-4844 blobs on testnet:
- Blob = ZK receipt bytes (field-element aligned, BN254)
- Posted to L1 every epoch (128 blocks)
- Retention: 4096 epochs (~18 days)
- Verifier: reads blob, checks commitment matches on-chain anchor
