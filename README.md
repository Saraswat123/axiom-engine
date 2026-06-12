# axiom-engine

> Proof-augmented AI agent engine in Rust. Every answer comes with mathematical proof of correctness, cryptographic execution integrity, and ZK-verifiable computation.

[![CI](https://github.com/Saraswat123/axiom-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/Saraswat123/axiom-engine/actions)

---

## Problem Statement

AI systems today are black boxes. You ask a question, get an answer, and have no way to verify:

- **Is the computation correct?** A model hallucinating a math result looks identical to a correct one.
- **Did the computation actually execute as claimed?** No audit trail, no replay, no proof.
- **Can a third party verify the result without re-running it?** No — every verifier must trust the system.
- **Is the system protected from adversarial inputs?** Formally, no — current systems have no domain invariants enforced at runtime.

This matters most in high-stakes domains: protocol design, financial computation, cryptographic circuit validation, on-chain settlement, and multi-agent coordination where individual agent trust cannot be assumed.

**axiom-engine** solves this by attaching machine-checkable proof to every computation:

| Problem | axiom-engine Answer |
|---|---|
| Answer could be hallucinated | Z3 SMT formal proof — every claim is logically verified |
| Execution is invisible | RISC Zero ZK receipt — cryptographic proof computation ran correctly |
| No replay capability | Deterministic trace hash — any node can re-derive the same result |
| No runtime invariants | Formal invariant registry — Z3 checks domain bounds on every call |
| No multi-party trust | P2P proof broadcast + AggregatedProof composition (Phase 6) |
| Confidential but auditable | AWS Nitro TEE attestation anchor (Phase 7) |

The result: a computation engine where **trust is replaced by proof**.

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

## Usability Test Cases

Manual test suite — run top to bottom. Each test has input, expected output, and what it verifies.

### Prerequisites

```bash
# Start engine
docker compose --profile engine up -d
# OR local
AXIOM_TRANSPORT=http PORT=8080 cargo run -p axiom-mcp-server
```

---

### TC-01 — Engine health check

```bash
curl -sf http://localhost:8080/health
```

**Expected:**
```json
{"engine":"axiom-engine","status":"ok","version":"0.1.0"}
```
**Verifies:** container running, HTTP2 transport alive.

---

### TC-02 — Expression optimization (egg)

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"egg_optimize","input":{"expression":"(+ x 0)"}}'
```

**Expected:**
```json
{"ok":true,"result":{"original":"(+ x 0)","optimized":"x"}}
```
**Verifies:** equality saturation reduces `(+ x 0)` to `x`.

---

### TC-03 — Z3 formal proof (passes)

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"z3_prove","input":{"property":"square_positive","low":1,"high":1000}}'
```

**Expected:**
```json
{"ok":true,"result":{"verdict":"proved","detail":"property holds for all values in range"}}
```
**Verifies:** Z3 proves x² > 0 for all x in [1,1000].

---

### TC-04 — Z3 formal proof (should prove edge: negative range)

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"z3_prove","input":{"property":"square_positive","low":-100,"high":-1}}'
```

**Expected:** `"verdict":"proved"` — x² > 0 holds even for negative x.
**Verifies:** Z3 handles negative domain correctly.

---

### TC-05 — OptVerify pipeline (first call — no cache)

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"opt_verify","input":{"expression":"(* x 1)"}}'
```

**Expected:**
```json
{
  "ok": true,
  "result": {
    "original":"(* x 1)",
    "optimized":"x",
    "z3_verdict":"proved",
    "cache_hit":false
  }
}
```
**Verifies:** egg + Z3 pipeline runs, result stored.

---

### TC-06 — OptVerify cache hit (repeat call)

Run **TC-05 again** (same expression).

**Expected:** `"cache_hit":true` — result served from sled store, Z3 not re-run.
**Verifies:** persistent DashMap+sled cache working.

---

### TC-07 — Matrix determinant

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"compute_matrix","input":{"op":"determinant","data":[2,0,0,3]}}'
```

**Expected:** `{"determinant":6.0}` — det([[2,0],[0,3]]) = 6.
**Verifies:** nalgebra compute correct.

---

### TC-08 — Matrix BN254 field blob (ZK-aligned)

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"compute_matrix","input":{"op":"field_blob","data":[1,2,3,4]}}'
```

**Expected:**
```json
{"field":"BN254","blob_len":128,"blob_hex":"..."}
```
**Verifies:** ark-ff BN254 field encoding produces 32 bytes per element.

---

### TC-09 — Execution trace grows

```bash
# Step 1: check initial trace length
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"trace_snapshot","input":{}}' | python3 -c "import sys,json; d=json.load(sys.stdin); print('entries:', len(d['result']['entries']))"

# Step 2: run opt_verify
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"opt_verify","input":{"expression":"(+ x 0)"}}' > /dev/null

# Step 3: trace should have grown
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"trace_snapshot","input":{}}' | python3 -c "import sys,json; d=json.load(sys.stdin); print('entries:', len(d['result']['entries']))"
```

**Expected:** entry count increases by 2 (egg call + z3 call recorded).
**Verifies:** TraceRecorder captures all tool calls deterministically.

---

### TC-10 — Persistence across container restart

```bash
# Step 1: populate cache
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"opt_verify","input":{"expression":"(* x 0)"}}' > /dev/null

# Step 2: check count
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"store_stats","input":{}}' | python3 -m json.tool

# Step 3: restart container
docker restart axiom-engine-axiom-engine-1 && sleep 4

# Step 4: count must be same (sled persistence)
curl -sf -X POST http://localhost:8080/tools \
  -d '{"tool":"store_stats","input":{}}' | python3 -m json.tool
```

**Expected:** `total` same before and after restart, `persistent: true`.
**Verifies:** sled L2 store survives container restart via Docker volume.

---

### TC-11 — Store stats structure

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"store_stats","input":{}}'
```

**Expected:**
```json
{"ok":true,"result":{"total":N,"persistent":true,"by_kind":{"SmtResult":N}}}
```
**Verifies:** store reports persistent=true (sled open), kind breakdown correct.

---

### TC-12 — MCP JSON-RPC protocol

```bash
curl -sf -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

**Expected:** JSON-RPC 2.0 response with `result.tools` array, 6 tools listed.
**Verifies:** MCP protocol compliance (Claude Code / Copilot can connect).

---

### TC-13 — Unknown tool returns error

```bash
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"nonexistent","input":{}}'
```

**Expected:** `{"ok":false,"error":"unknown tool: nonexistent"}` with HTTP 400.
**Verifies:** error handling, no panic on bad input.

---

### TC-14 — ZK proof dev mode (requires risc0 build)

```bash
RISC0_DEV_MODE=1 bash scripts/run-zk.sh &
sleep 10
curl -sf -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{"tool":"zk_prove","input":{"op":"sum","data":[1,2,3,4,5]}}'
```

**Expected:**
```json
{
  "status":"proved",
  "op":"sum",
  "result":15.0,
  "verified":true,
  "dev_mode":true
}
```
**Verifies:** RISC Zero guest runs, receipt generated, commitment produced.

---

### TC-15 — Dashboard UI loads

```bash
curl -sf http://localhost:8080/ | grep "AXIOM-ENGINE"
curl -sf http://localhost:8080/logs | grep "LIVE LOGS"
```

**Expected:** both return HTML with matching title strings.
**Verifies:** dashboard + live log viewer routes active.

---

### Unit Test Suite

```bash
# All unit tests
cargo test --all

# Expected output:
# axiom-compute:   test_identity_det ✓  test_field_encoding ✓
# axiom-egg-tool:  test_simplify_add_zero ✓  test_simplify_mul_one ✓
# axiom-z3-tool:   test_square_positive ✓
# axiom-pipeline:  test_opt_verify_add_zero ✓
```

---

### Integration Test Suite (requires running server)

```bash
AXIOM_TRANSPORT=http cargo run -p axiom-mcp-server &
sleep 3
cargo test --test integration -- --ignored --test-threads=1
```

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
