use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofBundle {
    pub result: serde_json::Value,
    pub z3_verdict: String,
    pub egg_optimized: Option<String>,
    pub zk_receipt: Option<String>,
}
