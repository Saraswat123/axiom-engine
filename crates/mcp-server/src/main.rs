use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde_json::{json, Value};
use tracing::{info, error};
use axiom_z3_tool::Z3Tool;
use axiom_egg_tool::EggTool;
use axiom_compute::ComputeTool;

// MCP server over stdio transport
// Protocol: newline-delimited JSON (JSON-RPC 2.0)

struct Server {
    z3: Z3Tool,
    egg: EggTool,
    compute: ComputeTool,
}

impl Server {
    fn new() -> Self {
        Self {
            z3: Z3Tool::new(),
            egg: EggTool::new(),
            compute: ComputeTool::new(),
        }
    }

    async fn handle(&self, req: Value) -> Value {
        let id = req["id"].clone();
        let method = req["method"].as_str().unwrap_or("");
        let params = req["params"].clone();

        match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": { "name": "axiom-engine", "version": "0.1.0" },
                    "capabilities": { "tools": {} }
                }
            }),

            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "z3_prove",
                            "description": "Formally verify a mathematical property using Z3 SMT solver",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "property": { "type": "string" },
                                    "low": { "type": "integer" },
                                    "high": { "type": "integer" }
                                },
                                "required": ["property"]
                            }
                        },
                        {
                            "name": "egg_optimize",
                            "description": "Optimize a mathematical expression using equality saturation",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "expression": { "type": "string" }
                                },
                                "required": ["expression"]
                            }
                        },
                        {
                            "name": "compute_matrix",
                            "description": "Perform matrix computation using nalgebra with rayon parallelism",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "op": { "type": "string", "enum": ["determinant", "trace", "norm", "eigenvalues"] },
                                    "data": { "type": "array", "items": { "type": "number" } }
                                },
                                "required": ["op", "data"]
                            }
                        }
                    ]
                }
            }),

            "tools/call" => {
                let tool_name = params["name"].as_str().unwrap_or("");
                let tool_input = params["arguments"].clone();

                let result = match tool_name {
                    "z3_prove" => self.z3.prove(tool_input).await,
                    "egg_optimize" => self.egg.optimize(tool_input).await,
                    "compute_matrix" => self.compute.run(tool_input).await,
                    _ => Err(anyhow::anyhow!("unknown tool: {}", tool_name)),
                };

                match result {
                    Ok(v) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "content": [{ "type": "text", "text": v.to_string() }] }
                    }),
                    Err(e) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": e.to_string() }
                    }),
                }
            }

            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "method not found" }
            }),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("axiom-engine MCP server starting on stdio");

    let server = Server::new();
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                let resp = server.handle(req).await;
                let mut out = resp.to_string();
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
            Err(e) => error!("invalid JSON: {}", e),
        }
    }

    Ok(())
}
