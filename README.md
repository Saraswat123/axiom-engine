# axiom-engine

Proof-augmented AI agent engine in Rust. Every answer comes with mathematical proof of correctness.

## Stack

| Crate | Role |
|-------|------|
| `axiom-agent` | Claude agent loop via Anthropic API |
| `axiom-z3-tool` | Formal verification — Z3 SMT solver |
| `axiom-egg-tool` | Expression optimization — equality saturation |
| `axiom-compute` | Parallel matrix computation — nalgebra + rayon |
| `axiom-zk-proof` | ZK execution proofs — RISC Zero (Phase 5) |
| `axiom-mcp-server` | MCP server — exposes all tools to any agent |

## Pipeline

```
Claude agent receives task
  → Z3 verifies logical correctness
  → egg optimizes expression form
  → nalgebra + rayon executes computation
  → RISC Zero generates ZK proof (Phase 5)
  → returns: result + formal proof + execution receipt
```

## Build phases

- [x] Phase 1: Z3 tool + egg tool + compute tool
- [x] Phase 2: Agent loop + tool dispatch
- [x] Phase 3: MCP server (stdio transport)
- [ ] Phase 4: Full pipeline integration
- [ ] Phase 5: RISC Zero ZK proofs
- [ ] Phase 6: Post-quantum key exchange (Kyber)
- [ ] Phase 7: AWS Nitro TEE attestation

## Quick start

```bash
# Requires libz3-dev
# macOS: brew install z3
# Ubuntu: apt install libz3-dev

export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p axiom-agent

# Run MCP server
cargo run -p axiom-mcp-server
```

## Run tests

```bash
cargo test --all
```

## Contributors

- [@Saraswat123](https://github.com/Saraswat123)
- Claude Sonnet 4.6 (Anthropic) — AI pair programmer
- GitHub Copilot — inline suggestions

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
