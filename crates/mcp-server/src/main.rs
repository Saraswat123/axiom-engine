use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
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
        .route("/mcp", post(mcp_handler))
        .route("/tools", post(tool_call_handler))
        .route("/health", axum::routing::get(health))
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
