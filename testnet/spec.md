# Axiom Engine — Testnet Specification

## Stack (all phases complete)

| Phase | Component | Status |
|-------|-----------|--------|
| 1–4 | Z3 + egg + compute + pipeline + store + trace | ✅ |
| 5 | RISC Zero ZK proof generation | ✅ |
| 6 | libp2p P2P proof broadcast (gossipsub + Kademlia) | ✅ |
| 7 | Post-quantum identity (ML-KEM-768 + ML-DSA-65) | ✅ |
| 8 | AWS Nitro TEE attestation anchor | ✅ |

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Network ID | 42161337 (axiom-devnet) |
| Consensus | PoA (dev mode) → BLS PoS (testnet) |
| Block time | 2s |
| Gas limit | 30,000,000 |
| Native token | AXM (18 decimals) |

---

## Phase T1 — Single Node (current default)

Full proof-native stack on one machine. No P2P.

```bash
# Local
AXIOM_TRANSPORT=http PORT=8080 cargo run -p axiom-mcp-server

# Docker
docker compose --profile engine up
```

Endpoints live:
- `GET /health` — engine status
- `POST /tools` — all tools (z3_prove, zk_prove, pq_sign, tee_attest, ...)
- `GET /pq/status` — ML-DSA-65 + ML-KEM-768 identity
- `GET /tee/status` — Nitro TEE attestation status
- `GET /p2p/status` — P2P swarm (disabled in single-node)

---

## Phase T2 — 2/3-Node P2P Cluster

Verified locally and in CI. Gossipsub broadcasts ZK proof commitments
across all nodes the moment they are generated.

### 2-Node Local Test (verified ✅)

```bash
# Terminal 1 — Node 1 bootstrap
AXIOM_TRANSPORT=http PORT=8080 AXIOM_P2P_ENABLED=1 AXIOM_P2P_PORT=9000 \
  cargo run -p axiom-mcp-server

# Get peer ID
PEER1=$(curl -sf http://localhost:8080/p2p/status | python3 -c \
  "import sys,json; print(json.load(sys.stdin)['peer_id'])")

# Terminal 2 — Node 2 dials Node 1
AXIOM_TRANSPORT=http PORT=8081 AXIOM_P2P_ENABLED=1 AXIOM_P2P_PORT=9001 \
  AXIOM_PEERS="/ip4/127.0.0.1/tcp/9000/p2p/$PEER1" \
  cargo run -p axiom-mcp-server

# Verify peers=1 on both
curl -sf http://localhost:8080/p2p/status
curl -sf http://localhost:8081/p2p/status

# Broadcast proof commitment from Node 1
curl -X POST http://localhost:8080/tools \
  -d '{"tool":"p2p_broadcast","input":{"topic":"proof","payload":"my-commitment"}}'
# Node 2 logs: gossipsub message received topic="axiom/proof/v1"
```

### 3-Node Docker Cluster

```bash
docker compose --profile cluster up

# Node 1: localhost:8080  P2P: 9000
# Node 2: localhost:8081  P2P: 9001
# Node 3: localhost:8082  P2P: 9002
```

### Gossipsub Topics

| Topic | Purpose |
|-------|---------|
| `axiom/proof/v1` | ZK receipt commitments — auto-broadcast on every `zk_prove` |
| `axiom/prove-req/v1` | Request distributed proof generation from cluster |
| `axiom/trace/v1` | Execution trace hash gossip (deterministic audit trail) |
| `axiom/invariant/v1` | Invariant registry updates (Z3 verdicts) |

### P2P Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AXIOM_P2P_ENABLED` | `0` | Set to `1` to start swarm |
| `AXIOM_P2P_PORT` | `9000` | TCP listen port |
| `AXIOM_PEERS` | — | Comma-separated multiaddrs to dial on boot |

---

## Phase T3 — Public Testnet

- 10 validator nodes
- Espresso shared sequencer integration
- L2 rollup: Arbitrum Orbit (settles to axiom L1)
- Explorer: axiom-testnet.explorer (TBD)

---

## Smart Contracts

### ProofAnchor.sol

```solidity
// Anchors ZK proof receipts + TEE attestation document hash on-chain.
// axiom-engine posts commitment after every proven + attested computation.
interface IProofAnchor {
    function anchor(bytes32 commitment, bytes32 teeDocHash, bytes32 pcr0) external;
    function verify(bytes32 commitment) external view returns (bool, bytes32 pcr0);
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

---

## Running Tests

```bash
# All unit tests (excludes RISC-V guest)
cargo test --workspace --exclude axiom-guest

# P2P testnet (2 nodes, gossipsub)
# → runs automatically in CI as `p2p-testnet` job

# TEE + PQ smoke
# → runs automatically in CI as `tee-pq-smoke` job

# Integration tests (requires running server)
AXIOM_TRANSPORT=http cargo run -p axiom-mcp-server &
sleep 3
cargo test --test integration -- --test-threads=1
```

---

## CI Jobs

| Job | Trigger | What it tests |
|-----|---------|---------------|
| `build-test` | push + PR | fmt, clippy, all unit tests |
| `p2p-testnet` | push + PR | 2-node P2P cluster, gossipsub bidirectional |
| `tee-pq-smoke` | push + PR | TEE status, PQ sign/verify, TEE attest |
| `ai-review` | PR only | Claude code review posted as PR comment |

---

## Data Availability (Phase T3)

Proof receipts stored as EIP-4844 blobs on testnet:
- Blob = ZK receipt bytes (field-element aligned, BN254)
- Posted to L1 every epoch (128 blocks)
- Retention: 4096 epochs (~18 days)
- Verifier: reads blob, checks commitment matches on-chain anchor + TEE PCR0
