use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Deterministic execution trace — every tool call recorded with inputs/outputs.
/// Enables ZK guest replay: feed trace into RISC Zero, get proof the same
/// sequence of operations occurred.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub tool: String,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub duration_us: u64,
    pub proof_ref: Option<String>, // artifact ID from store
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub session_id: String,
    pub entries: Vec<TraceEntry>,
}

impl ExecutionTrace {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            entries: Vec::new(),
        }
    }

    /// Replay produces a hash that RISC Zero guest can verify deterministically.
    pub fn replay_hash(&self) -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        for entry in &self.entries {
            entry.tool.hash(&mut h);
            entry.input.to_string().hash(&mut h);
            entry.output.to_string().hash(&mut h);
            entry.seq.hash(&mut h);
        }
        // Expand u64 hash to [u8; 32] — replace with SHA-256 in Phase 5
        let v = h.finish();
        let mut out = [0u8; 32];
        out[..8].copy_from_slice(&v.to_le_bytes());
        out
    }
}

/// Thread-safe trace recorder — shared across all tool calls in a session.
#[derive(Clone)]
pub struct TraceRecorder {
    inner: Arc<RwLock<ExecutionTrace>>,
}

impl TraceRecorder {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExecutionTrace::new())),
        }
    }

    pub async fn record<F, Fut>(
        &self,
        tool: &str,
        input: serde_json::Value,
        f: F,
    ) -> Result<serde_json::Value>
    where
        F: FnOnce(serde_json::Value) -> Fut,
        Fut: std::future::Future<Output = Result<serde_json::Value>>,
    {
        let start = std::time::Instant::now();
        let output = f(input.clone()).await?;
        let duration_us = start.elapsed().as_micros() as u64;

        let mut trace = self.inner.write().await;
        let seq = trace.entries.len() as u64;
        let session_id = trace.session_id.clone();

        trace.entries.push(TraceEntry {
            id: Uuid::new_v4().to_string(),
            session_id,
            seq,
            timestamp: Utc::now(),
            tool: tool.to_string(),
            input,
            output: output.clone(),
            duration_us,
            proof_ref: None,
        });

        tracing::debug!(tool, seq, duration_us, "trace entry recorded");
        Ok(output)
    }

    pub async fn snapshot(&self) -> ExecutionTrace {
        self.inner.read().await.clone()
    }
}
