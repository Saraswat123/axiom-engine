# axiom-engine

> Proof-augmented AI agent engine in Rust. Every answer comes with mathematical proof of correctness, cryptographic execution integrity, and ZK-verifiable computation.

[![CI](https://github.com/Saraswat123/axiom-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/Saraswat123/axiom-engine/actions)

---

## What This Is

Most AI systems return answers. axiom-engine returns answers + proof.

```
Input: "optimize and verify: (+ x 0)"

Output:
  optimized:   x
  z3_verdict:  proved        ← mathematical guarantee
  trace_hash:  0x3f7a...     ← deterministic replay hash
  zk_receipt:  [Phase 5]     ← cryptographic execution proof
  cache_hit:   false         ← first call; subsequent = instant
```

Three guarantee layers:
- **Z3 SMT** — logical correctness for all inputs in domain
- **RISC Zero ZK** — cryptographic proof computation ran correctly (Phase 5)
- **Nitro TEE** — confidential execution, attestation anchor on-chain (Phase 7)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        AGENT LAYER                               │
│  Multi-model: Claude / Ollama / OpenAI / HuggingFace             │
│  Set: AXIOM_PROVIDER=ollama AXIOM_MODEL=llama3                   │
└──────────────────┬──────────────────────────────────────────────┘
                   │ tool calls
┌──────────────────▼──────────────────────────────────────────────┐
│                      PIPELINE LAYER                              │
│  OptVerify: egg optimization → Z3 soundness (one call, cached)  │
│  TraceRecorder: every call logged → deterministic replay hash    │
│  ArtifactStore: DashMap cache — zero redundant computation       │
└──────┬────────────────┬──────────────────┬──────────────────────┘
       │                │                  │
┌──────▼──────┐  ┌──────▼──────┐  ┌───────▼──────────────────────┐
│  z3-tool    │  │  egg-tool   │  │  compute                      │
│  SMT solver │  │  Equality   │  │  nalgebra + rayon (parallel)  │
│  Z3 12.x    │  │  saturation │  │  ark-ff BN254 field blobs     │
│             │  │  cached     │  │  ZK-circuit aligned math      │
└─────────────┘  └─────────────┘  └──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────────┐
│                      ZK PROOF LAYER                              │
│  ProofBundle trait — all proof types implement this             │
│  AggregatedProof — compose N sub-proofs into one                │
│  RISC Zero zkVM — Phase 5                                        │
└──────────────────┬──────────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────────┐
│                    TRANSPORT LAYER                               │
│  HTTP2 (axum) — high-frequency tool calls, no stdio bottleneck  │
│  stdio MCP — local Claude Code / desktop compatibility          │
│  libp2p P2P — proof broadcast across nodes (Phase 6)            │
└─────────────────────────────────────────────────────────────────┘
```

---

## Workspace Structure

```
axiom-engine/
├── crates/
│   ├── agent/           Claude agent loop — model-agnostic
│   ├── z3-tool/         Z3 SMT solver — formal verification
│   ├── egg-tool/        Equality saturation — expression optimizer
│   ├── compute/         nalgebra + rayon + ark-ff BN254 field math
│   ├── zk-proof/        ProofBundle trait + AggregatedProof
│   ├── pipeline/        OptVerify combinator (egg→Z3, cached)
│   ├── store/           DashMap artifact cache + field blob storage
│   ├── trace/           Deterministic replay log for ZK guest
│   ├── p2p-transport/   libp2p swarm scaffold (Phase 6)
│   └── mcp-server/      axum HTTP2 + stdio MCP server
├── guest/               RISC Zero guest (Phase 5)
├── tests/               Integration test suite (HTTP)
├── testnet/             Devnet → public testnet spec
├── .agents/
│   ├── registry.yml     Agent boundary ownership map
│   └── invariants.yml   Formal invariant registry (6 invariants)
├── .github/
│   ├── workflows/ci.yml CI + Claude auto-review on PRs
│   └── scripts/         claude_review.py
├── Dockerfile           Multi-stage, minimal runtime
└── docker-compose.yml   Engine + local testnet + prometheus
```

---

## Quick Start

### Prerequisites

```bash
# macOS
brew install z3 rust

# Ubuntu
apt install libz3-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Run (stdio MCP — default)

```bash
git clone https://github.com/Saraswat123/axiom-engine
cd axiom-engine
export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p axiom-mcp-server
```

### Run (HTTP2)

```bash
AXIOM_TRANSPORT=http PORT=8080 cargo run -p axiom-mcp-server
curl http://localhost:8080/health
```

### Run with Docker

```bash
docker-compose up axiom-engine
curl http://localhost:8080/health
```

### Run Tests

```bash
cargo test --all
```

---

## Tool API (HTTP)

**POST /tools**

```bash
# OptVerify — egg optimization + Z3 soundness in one call (cached)
curl -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"opt_verify","input":{"expression":"(+ x 0)"}}'

# Response:
# { "ok": true, "result": {
#     "original": "(+ x 0)", "optimized": "x",
#     "z3_verdict": "proved", "cache_hit": false
# }}

# Z3 formal proof
curl -X POST http://localhost:8080/tools \
  -d '{"tool":"z3_prove","input":{"property":"square_positive","low":1,"high":1000}}'

# Matrix compute — BN254 field blob (ZK-aligned)
curl -X POST http://localhost:8080/tools \
  -d '{"tool":"compute_matrix","input":{"op":"field_blob","data":[1,0,0,1]}}'

# Execution trace (for ZK replay)
curl -X POST http://localhost:8080/tools -d '{"tool":"trace_snapshot","input":{}}'

# Cache stats
curl -X POST http://localhost:8080/tools -d '{"tool":"store_stats","input":{}}'
```

---

## Multi-Model Support

```bash
# Claude (default)
AXIOM_PROVIDER=anthropic AXIOM_MODEL=claude-sonnet-4-6 cargo run -p axiom-agent

# Local Ollama (no API key needed)
AXIOM_PROVIDER=ollama AXIOM_MODEL=llama3.2 OLLAMA_URL=http://localhost:11434 cargo run -p axiom-agent

# OpenAI
AXIOM_PROVIDER=openai AXIOM_MODEL=gpt-4o cargo run -p axiom-agent

# HuggingFace
AXIOM_PROVIDER=huggingface AXIOM_MODEL=mistralai/Mistral-7B-Instruct-v0.3 cargo run -p axiom-agent
```

---

## Formal Invariant System

`.agents/invariants.yml` defines 6 machine-readable invariants Z3 verifies.
CI blocks merge if any critical invariant fails verification.

```yaml
- id: INV-001  # x^2 >= 0 for all real x
- id: INV-002  # BN254 field element bounds
- id: INV-003  # trace sequence monotonically increases
- id: INV-004  # proof bundle non-empty
- id: INV-005  # matrix computation deterministic
- id: INV-006  # content-addressed store keys collision-free
```

---

## Build Phases

| Phase | Status | Description |
|-------|--------|-------------|
| 1 | ✅ | Z3 + egg + compute tools |
| 2 | ✅ | Agent loop (multi-model) |
| 3 | ✅ | MCP server HTTP2 + stdio |
| 4 | ✅ | Pipeline + store + trace |
| 5 | 🔲 | RISC Zero ZK proof generation |
| 6 | 🔲 | libp2p P2P proof broadcast |
| 7 | 🔲 | Post-quantum keys (Kyber/Dilithium) |
| 8 | 🔲 | AWS Nitro TEE attestation anchor |

---

## Testnet

See [`testnet/spec.md`](testnet/spec.md) for:
- Local devnet setup (Geth --dev)
- 3-node cluster (docker-compose)
- Public testnet parameters
- EIP-4844 blob DA for proof receipts

---

## Target Companies

This stack (Rust + ZK + Formal Verification + AI Agents) is the exact intersection hired for at:
**Anthropic · RISC Zero · Succinct Labs · Trail of Bits · AWS Kani team · ArcadeAI · Cloudflare**

---

## Contributors

- [@Saraswat123](https://github.com/Saraswat123)
- Claude Sonnet 4.6 (Anthropic) — AI pair programmer

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
