use crate::AppState;
use serde_json::{json, Value};

pub async fn handle(state: &AppState, req: Value) -> Value {
    let id = req["id"].clone();
    let method = req["method"].as_str().unwrap_or("");
    let params = req["params"].clone();

    match method {
        "initialize" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "axiom-engine", "version": "0.1.0" },
                "capabilities": { "tools": {} }
            }
        }),

        "tools/list" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": { "tools": tools_schema() }
        }),

        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let input = params["arguments"].clone();
            match state.dispatch(name, input).await {
                Ok(v) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": v.to_string() }] }
                }),
                Err(e) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32000, "message": e.to_string() }
                }),
            }
        }

        _ => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": "method not found" }
        }),
    }
}

fn tools_schema() -> Value {
    json!([
        {
            "name": "z3_prove",
            "description": "Formally verify property via Z3 SMT solver",
            "inputSchema": { "type": "object", "properties": {
                "property": { "type": "string" },
                "low": { "type": "integer" }, "high": { "type": "integer" }
            }, "required": ["property"] }
        },
        {
            "name": "egg_optimize",
            "description": "Optimize expression via equality saturation",
            "inputSchema": { "type": "object", "properties": {
                "expression": { "type": "string" }
            }, "required": ["expression"] }
        },
        {
            "name": "opt_verify",
            "description": "egg optimization + Z3 soundness check in one pipeline (cached)",
            "inputSchema": { "type": "object", "properties": {
                "expression": { "type": "string" }
            }, "required": ["expression"] }
        },
        {
            "name": "compute_matrix",
            "description": "Matrix computation — supports field_blob for ZK circuit input",
            "inputSchema": { "type": "object", "properties": {
                "op": { "type": "string", "enum": ["determinant","trace","norm","eigenvalues","field_blob"] },
                "data": { "type": "array", "items": { "type": "number" } }
            }, "required": ["op", "data"] }
        },
        {
            "name": "store_stats",
            "description": "Artifact cache statistics",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "trace_snapshot",
            "description": "Current deterministic execution trace (for ZK replay)",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}
