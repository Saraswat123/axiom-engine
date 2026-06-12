use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
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
        .route("/logs", get(logs_page))
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

async fn logs_page() -> impl IntoResponse {
    Html(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>axiom-engine — logs</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:'Courier New',monospace;background:#080c10;color:#c9d1d9;padding:1rem}
header{display:flex;align-items:center;gap:1rem;margin-bottom:1rem;border-bottom:1px solid #21262d;padding-bottom:.75rem}
.logo{color:#58a6ff;font-weight:bold;font-size:1.2rem}
a{color:#58a6ff;text-decoration:none}
.badge{background:#1a2f4a;color:#58a6ff;border:1px solid #30363d;padding:2px 8px;border-radius:10px;font-size:.75rem}
#log-box{background:#010409;border:1px solid #21262d;border-radius:6px;padding:1rem;height:calc(100vh - 120px);overflow-y:auto;font-size:.82rem;line-height:1.6}
.INFO{color:#3fb950}.WARN{color:#d29922}.ERROR{color:#f85149}.DEBUG{color:#6e7681}
.ts{color:#30363d;margin-right:.5rem}
#status{font-size:.75rem;color:#6e7681;margin-left:auto}
</style>
</head>
<body>
<header>
  <span class="logo">AXIOM-ENGINE</span>
  <span class="badge">LIVE LOGS</span>
  <a href="/">← dashboard</a>
  <span id="status">connecting...</span>
</header>
<div id="log-box"></div>
<script>
const box = document.getElementById('log-box');
const status = document.getElementById('status');
let count = 0;

function addLine(line) {
  const div = document.createElement('div');
  // Detect log level
  const level = line.includes(' INFO ') ? 'INFO'
    : line.includes(' WARN ') ? 'WARN'
    : line.includes(' ERROR ') ? 'ERROR'
    : line.includes(' DEBUG ') ? 'DEBUG' : '';
  if (level) div.className = level;
  div.textContent = line;
  box.appendChild(div);
  box.scrollTop = box.scrollHeight;
  count++;
  status.textContent = `${count} lines — ${new Date().toLocaleTimeString()}`;
}

// Poll trace_snapshot every 2s for activity log
async function pollTrace() {
  try {
    const r = await fetch('/tools', {
      method: 'POST',
      headers: {'Content-Type':'application/json'},
      body: JSON.stringify({tool:'trace_snapshot',input:{}})
    });
    const d = await r.json();
    const entries = d.result?.entries || [];
    if (entries.length !== lastCount) {
      entries.slice(lastCount).forEach(e => {
        addLine(`[${e.timestamp || new Date().toISOString()}] INFO  ${e.tool} input=${JSON.stringify(e.input)} duration=${e.duration_us}µs`);
      });
      lastCount = entries.length;
    }
  } catch(e) {}
}

// Also poll store stats
async function pollStats() {
  try {
    const r = await fetch('/tools', {
      method: 'POST',
      headers: {'Content-Type':'application/json'},
      body: JSON.stringify({tool:'store_stats',input:{}})
    });
    const d = await r.json();
    const t = d.result?.total || 0;
    if (t !== lastCache) {
      addLine(`[${new Date().toISOString()}] INFO  store cache_total=${t} persistent=true`);
      lastCache = t;
    }
  } catch(e) {}
}

let lastCount = 0, lastCache = 0;

// Initial engine start log
addLine(`[${new Date().toISOString()}] INFO  axiom-engine HTTP server running on 0.0.0.0:8080`);
addLine(`[${new Date().toISOString()}] INFO  transport=HTTP2 store=persistent zk=phase-5`);
status.textContent = 'live';

setInterval(pollTrace, 2000);
setInterval(pollStats, 3000);
</script>
</body>
</html>"#)
}

async fn dashboard(_state: State<AppState>) -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}

static DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>axiom-engine</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:'Courier New',monospace;background:#080c10;color:#c9d1d9;min-height:100vh}
header{background:linear-gradient(135deg,#0d1117 0%,#161b22 100%);border-bottom:1px solid #21262d;padding:1.5rem 2rem;display:flex;align-items:center;gap:1rem}
.logo{font-size:2rem;font-weight:bold;color:#58a6ff;letter-spacing:2px}
.logo span{color:#3fb950}
.badge{display:inline-block;padding:2px 10px;border-radius:12px;font-size:.75rem;font-weight:bold;margin-left:.5rem}
.badge.green{background:#1a4428;color:#3fb950;border:1px solid #3fb950}
.badge.blue{background:#1a2f4a;color:#58a6ff;border:1px solid #58a6ff}
main{padding:2rem;max-width:1200px;margin:0 auto}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem;margin:1.5rem 0}
.card{background:#0d1117;border:1px solid #21262d;border-radius:8px;padding:1.25rem}
.card h3{color:#58a6ff;margin-bottom:.75rem;font-size:.9rem;text-transform:uppercase;letter-spacing:1px}
.stat-val{font-size:2.5rem;font-weight:bold;color:#3fb950}
.stat-label{font-size:.8rem;color:#6e7681;margin-top:.25rem}
.tool-row{display:flex;align-items:center;gap:.75rem;padding:.5rem 0;border-bottom:1px solid #161b22;cursor:pointer}
.tool-row:last-child{border-bottom:none}
.tool-row:hover{background:#161b22;margin:0 -.75rem;padding:.5rem .75rem;border-radius:4px}
.tool-dot{width:8px;height:8px;border-radius:50%;background:#3fb950;flex-shrink:0}
.tool-name{color:#58a6ff;font-weight:bold;font-size:.9rem;min-width:140px}
.tool-desc{color:#8b949e;font-size:.8rem}
.phase-row{display:flex;align-items:center;gap:.75rem;padding:.4rem 0}
.phase-icon{font-size:1rem;width:20px;text-align:center}
.phase-done{color:#3fb950}
.phase-todo{color:#30363d}
.phase-label{font-size:.85rem}
section h2{color:#e6edf3;margin:1.5rem 0 .5rem;font-size:1.1rem;border-bottom:1px solid #21262d;padding-bottom:.5rem}
.tester{background:#0d1117;border:1px solid #21262d;border-radius:8px;padding:1.25rem;margin-top:1rem}
.tester select,.tester input,.tester textarea{background:#010409;border:1px solid #30363d;color:#c9d1d9;padding:.5rem .75rem;border-radius:6px;font-family:inherit;font-size:.875rem;width:100%;margin:.4rem 0}
.tester select:focus,.tester input:focus,.tester textarea:focus{outline:none;border-color:#58a6ff}
.btn{background:#238636;color:#fff;border:none;padding:.6rem 1.25rem;border-radius:6px;cursor:pointer;font-family:inherit;font-weight:bold;margin-top:.5rem}
.btn:hover{background:#2ea043}
.result{background:#010409;border:1px solid #21262d;border-radius:6px;padding:1rem;margin-top:.75rem;font-size:.85rem;white-space:pre-wrap;color:#3fb950;display:none;max-height:300px;overflow-y:auto}
.status-row{display:flex;gap:1rem;flex-wrap:wrap;margin:.5rem 0}
.status-chip{background:#161b22;border:1px solid #21262d;border-radius:6px;padding:.4rem .8rem;font-size:.8rem}
.status-chip span{color:#3fb950;font-weight:bold}
</style>
</head>
<body>
<header>
  <div class="logo">AXIOM<span>-ENGINE</span></div>
  <span class="badge green">RUNNING</span>
  <span class="badge blue">v0.1.0</span>
  <span class="badge blue">HTTP2</span>
  <a href="/logs" style="margin-left:auto;font-size:.8rem;color:#58a6ff;text-decoration:none">Live Logs →</a>
</header>
<main>
  <div class="status-row" id="status-row">
    <div class="status-chip">Cache items: <span id="cache-total">-</span></div>
    <div class="status-chip">Trace entries: <span id="trace-count">-</span></div>
    <div class="status-chip">Transport: <span>HTTP2 (axum)</span></div>
    <div class="status-chip">ZK: <span>Phase 5</span></div>
  </div>

  <div class="grid">
    <div class="card">
      <h3>Artifact Cache</h3>
      <div class="stat-val" id="stat-cache">-</div>
      <div class="stat-label">memoized results (DashMap)</div>
    </div>
    <div class="card">
      <h3>Trace Entries</h3>
      <div class="stat-val" id="stat-trace">-</div>
      <div class="stat-label">deterministic replay log</div>
    </div>
    <div class="card">
      <h3>Engine Stack</h3>
      <div style="font-size:.8rem;line-height:1.8;color:#8b949e">
        Z3 12.x &nbsp;·&nbsp; egg 0.9 &nbsp;·&nbsp; nalgebra 0.33<br>
        ark-ff BN254 &nbsp;·&nbsp; rayon &nbsp;·&nbsp; axum 0.7<br>
        Rust 1.93 &nbsp;·&nbsp; tokio async
      </div>
    </div>
  </div>

  <section>
    <h2>Tools</h2>
    <div class="card">
      <div class="tool-row" onclick="setTool('opt_verify','(+ x 0)')">
        <div class="tool-dot"></div>
        <div class="tool-name">opt_verify</div>
        <div class="tool-desc">egg optimization + Z3 soundness in one pipeline (cached)</div>
      </div>
      <div class="tool-row" onclick="setTool('z3_prove','square_positive')">
        <div class="tool-dot"></div>
        <div class="tool-name">z3_prove</div>
        <div class="tool-desc">formal SMT verification — returns proved / counterexample</div>
      </div>
      <div class="tool-row" onclick="setTool('egg_optimize','(* x 1)')">
        <div class="tool-dot"></div>
        <div class="tool-name">egg_optimize</div>
        <div class="tool-desc">equality saturation — finds minimal equivalent expression</div>
      </div>
      <div class="tool-row" onclick="setTool('compute_matrix','field_blob')">
        <div class="tool-dot"></div>
        <div class="tool-name">compute_matrix</div>
        <div class="tool-desc">nalgebra + rayon parallel math + BN254 field blobs for ZK</div>
      </div>
      <div class="tool-row" onclick="callTool('store_stats',{})">
        <div class="tool-dot"></div>
        <div class="tool-name">store_stats</div>
        <div class="tool-desc">artifact cache statistics</div>
      </div>
      <div class="tool-row" onclick="callTool('trace_snapshot',{})">
        <div class="tool-dot"></div>
        <div class="tool-name">trace_snapshot</div>
        <div class="tool-desc">current deterministic execution trace for ZK replay</div>
      </div>
    </div>
  </section>

  <section>
    <h2>Live Tool Tester</h2>
    <div class="tester">
      <select id="tool-select" onchange="onToolChange()">
        <option value="opt_verify">opt_verify</option>
        <option value="z3_prove">z3_prove</option>
        <option value="egg_optimize">egg_optimize</option>
        <option value="compute_matrix">compute_matrix</option>
        <option value="store_stats">store_stats</option>
        <option value="trace_snapshot">trace_snapshot</option>
      </select>
      <textarea id="tool-input" rows="4">{"expression": "(+ x 0)"}</textarea>
      <button class="btn" onclick="runTool()">Run Tool</button>
      <div class="result" id="tool-result"></div>
    </div>
  </section>

  <section>
    <h2>Build Phases</h2>
    <div class="card">
      <div class="phase-row"><span class="phase-icon phase-done">&#10003;</span><span class="phase-label">Phase 1-4 — Z3, egg, compute, pipeline, store, trace, HTTP2, Docker</span></div>
      <div class="phase-row"><span class="phase-icon phase-todo">&#9711;</span><span class="phase-label">Phase 5 — RISC Zero ZK proof generation</span></div>
      <div class="phase-row"><span class="phase-icon phase-todo">&#9711;</span><span class="phase-label">Phase 6 — libp2p P2P proof broadcast</span></div>
      <div class="phase-row"><span class="phase-icon phase-todo">&#9711;</span><span class="phase-label">Phase 7 — Post-quantum keys (Kyber / Dilithium)</span></div>
      <div class="phase-row"><span class="phase-icon phase-todo">&#9711;</span><span class="phase-label">Phase 8 — AWS Nitro TEE attestation anchor</span></div>
    </div>
  </section>

  <p style="margin-top:2rem;font-size:.8rem;color:#6e7681">
    <a href="https://github.com/Saraswat123/axiom-engine" style="color:#58a6ff">github.com/Saraswat123/axiom-engine</a>
    &nbsp;·&nbsp; <a href="/health" style="color:#58a6ff">GET /health</a>
  </p>
</main>

<script>
const TOOLS = {
  opt_verify:     '{"expression": "(+ x 0)"}',
  z3_prove:       '{"property": "square_positive", "low": 1, "high": 1000}',
  egg_optimize:   '{"expression": "(* x 1)"}',
  compute_matrix: '{"op": "field_blob", "data": [1,0,0,1]}',
  store_stats:    '{}',
  trace_snapshot: '{}'
};

function onToolChange() {
  const t = document.getElementById('tool-select').value;
  document.getElementById('tool-input').value = TOOLS[t] || '{}';
}

function setTool(name, hint) {
  document.getElementById('tool-select').value = name;
  onToolChange();
}

async function callTool(name, input) {
  document.getElementById('tool-select').value = name;
  document.getElementById('tool-input').value = JSON.stringify(input, null, 2);
  await runTool();
}

async function runTool() {
  const tool = document.getElementById('tool-select').value;
  let input;
  try { input = JSON.parse(document.getElementById('tool-input').value); }
  catch(e) { showResult('JSON parse error: ' + e.message); return; }

  showResult('Running...');
  try {
    const r = await fetch('/tools', {
      method: 'POST',
      headers: {'Content-Type':'application/json'},
      body: JSON.stringify({tool, input})
    });
    const data = await r.json();
    showResult(JSON.stringify(data, null, 2));
  } catch(e) { showResult('Error: ' + e.message); }
}

function showResult(text) {
  const el = document.getElementById('tool-result');
  el.style.display = 'block';
  el.textContent = text;
}

async function refreshStats() {
  try {
    const r = await fetch('/tools', {
      method: 'POST',
      headers: {'Content-Type':'application/json'},
      body: JSON.stringify({tool:'store_stats',input:{}})
    });
    const d = await r.json();
    const total = d.result?.total ?? 0;
    document.getElementById('stat-cache').textContent = total;
    document.getElementById('cache-total').textContent = total;

    const r2 = await fetch('/tools', {
      method: 'POST',
      headers: {'Content-Type':'application/json'},
      body: JSON.stringify({tool:'trace_snapshot',input:{}})
    });
    const d2 = await r2.json();
    const entries = d2.result?.entries?.length ?? 0;
    document.getElementById('stat-trace').textContent = entries;
    document.getElementById('trace-count').textContent = entries;
  } catch(e) {}
}

refreshStats();
setInterval(refreshStats, 5000);
</script>
</body>
</html>"#;

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
