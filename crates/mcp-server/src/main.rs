use anyhow::Result;
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower_http::trace::TraceLayer;
use tracing::info;

use axiom_compute::ComputeTool;
use axiom_egg_tool::EggTool;
use axiom_pipeline::OptVerify;
use axiom_store::ArtifactStore;
use axiom_trace::TraceRecorder;
use axiom_z3_tool::Z3Tool;
use axiom_zk_proof::ZkProofTool;

mod mcp;

#[derive(Clone)]
pub struct AppState {
    store: ArtifactStore,
    trace: TraceRecorder,
    z3: Arc<Z3Tool>,
    egg: Arc<EggTool>,
    compute: Arc<ComputeTool>,
    zk: Arc<ZkProofTool>,
}

impl AppState {
    fn new() -> Self {
        let store = ArtifactStore::new();
        let trace = TraceRecorder::new();
        Self {
            store,
            trace,
            z3: Arc::new(Z3Tool::new()),
            egg: Arc::new(EggTool::new()),
            compute: Arc::new(ComputeTool::new()),
            zk: Arc::new(ZkProofTool::new()),
        }
    }

    async fn dispatch(&self, name: &str, input: Value) -> Result<Value> {
        match name {
            "z3_prove" => self.z3.prove(input).await,
            "egg_optimize" => self.egg.optimize(input).await,
            "compute_matrix" => self.compute.run(input).await,
            "zk_prove" => self.zk.generate_proof(input).await,
            "opt_verify" => {
                let ov = OptVerify::new(self.store.clone(), self.trace.clone());
                let expr = input["expression"].as_str().unwrap_or("x");
                let result = ov.run(expr, None).await?;
                Ok(serde_json::to_value(result)?)
            }
            "store_stats" => {
                Ok(serde_json::to_value(self.store.stats())?)
            }
            "trace_snapshot" => {
                let snap = self.trace.snapshot().await;
                Ok(serde_json::to_value(snap)?)
            }
            _ => anyhow::bail!("unknown tool: {}", name),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let state = AppState::new();
    let transport = std::env::var("AXIOM_TRANSPORT").unwrap_or_else(|_| "stdio".to_string());

    match transport.as_str() {
        "http" => run_http(state).await,
        _ => run_stdio(state).await,
    }
}

/// HTTP2 transport — high-frequency tool calls, no stdio serialization bottleneck
async fn run_http(state: AppState) -> Result<()> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/mcp", post(mcp_handler))
        .route("/tools", post(tool_call_handler))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("axiom-engine HTTP server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_handler(
    State(state): State<AppState>,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let resp = mcp::handle(&state, req).await;
    Json(resp)
}

async fn tool_call_handler(
    State(state): State<AppState>,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let name = req["tool"].as_str().unwrap_or("");
    let input = req["input"].clone();
    match state.dispatch(name, input).await {
        Ok(v) => (StatusCode::OK, Json(json!({ "ok": true, "result": v }))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "engine": "axiom-engine", "version": "0.1.0" }))
}

async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.store.stats();
    Html(format!(r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>axiom-engine</title>
  <style>
    body {{ font-family: monospace; background: #0d0d0d; color: #e0e0e0; padding: 2rem; }}
    h1 {{ color: #7fff7f; }} h2 {{ color: #7fd4ff; margin-top: 2rem; }}
    .badge {{ display:inline-block; background:#1a3a1a; color:#7fff7f; border:1px solid #7fff7f; padding:2px 8px; border-radius:4px; }}
    .card {{ background:#1a1a1a; border:1px solid #333; border-radius:8px; padding:1rem; margin:0.5rem 0; }}
    pre {{ background:#111; padding:1rem; border-radius:6px; overflow-x:auto; color:#ccc; }}
    a {{ color:#7fd4ff; }}
  </style>
</head>
<body>
  <h1>⬡ axiom-engine <span class="badge">v0.1.0</span> <span class="badge">RUNNING</span></h1>
  <p>Proof-augmented AI agent engine in Rust — Z3 + egg + nalgebra + ZK</p>

  <h2>Status</h2>
  <div class="card">
    <b>Artifact cache:</b> {total} items &nbsp;|&nbsp;
    <b>Transport:</b> HTTP2 (axum) &nbsp;|&nbsp;
    <b>ZK:</b> Phase 5 (pending)
  </div>

  <h2>Available Tools</h2>
  <div class="card">
    <b>opt_verify</b> — egg optimization + Z3 soundness (cached)<br>
    <b>z3_prove</b> — formal SMT verification<br>
    <b>egg_optimize</b> — equality saturation<br>
    <b>compute_matrix</b> — nalgebra + BN254 field blobs<br>
    <b>store_stats</b> — artifact cache stats<br>
    <b>trace_snapshot</b> — deterministic replay trace
  </div>

  <h2>Quick Test</h2>
  <pre>curl -X POST http://localhost:8080/tools \
  -H "Content-Type: application/json" \
  -d '{{"tool":"opt_verify","input":{{"expression":"(+ x 0)"}}}}'</pre>

  <h2>Endpoints</h2>
  <div class="card">
    <a href="/health">GET /health</a> — liveness check<br>
    POST /tools — tool call API<br>
    POST /mcp — MCP JSON-RPC 2.0
  </div>

  <h2>Build Phases</h2>
  <div class="card">
    ✅ Phase 1-4 — Z3, egg, compute, pipeline, store, trace, HTTP2, Docker<br>
    🔲 Phase 5 — RISC Zero ZK proof generation<br>
    🔲 Phase 6 — libp2p P2P broadcast<br>
    🔲 Phase 7 — Post-quantum keys (Kyber/Dilithium)<br>
    🔲 Phase 8 — AWS Nitro TEE attestation
  </div>

  <p style="color:#555;margin-top:2rem;">
    <a href="https://github.com/Saraswat123/axiom-engine">github.com/Saraswat123/axiom-engine</a>
  </p>
</body>
</html>"#, total = stats.total))
}

/// stdio transport — MCP protocol (JSON-RPC 2.0, newline-delimited)
async fn run_stdio(state: AppState) -> Result<()> {
    info!("axiom-engine MCP server on stdio (set AXIOM_TRANSPORT=http for HTTP2)");
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                let resp = mcp::handle(&state, req).await;
                let mut out = resp.to_string();
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
            Err(e) => tracing::error!("invalid JSON: {}", e),
        }
    }
    Ok(())
}
