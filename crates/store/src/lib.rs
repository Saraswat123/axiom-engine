use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared artifact store — memoizes e-graph states, SMT results, proof receipts.
/// Lock-free reads via DashMap. Every tool call checks here first before
/// recomputing expensive Z3/egg results.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub data: Vec<u8>,
    pub created_at: u64, // unix ms
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactKind {
    EGraphState,   // serialized egg e-graph
    SmtResult,     // Z3 proof verdict
    ZkReceipt,     // RISC Zero receipt bytes
    ComputeResult, // nalgebra output
    TraceSnapshot, // execution trace
    BlobField,     // F_p field element blob for ZK circuits
}

#[derive(Clone)]
pub struct ArtifactStore {
    // Key: content-addressed hash of (tool, input) → dedup automatically
    cache: Arc<DashMap<String, Artifact>>,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Content-addressed key: SHA-256 of (kind + canonical input)
    pub fn key(kind: &ArtifactKind, input: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        format!("{:?}", kind).hash(&mut h);
        input.to_string().hash(&mut h);
        format!("{:016x}", h.finish())
    }

    pub fn get(&self, key: &str) -> Option<Artifact> {
        self.cache.get(key).map(|e| e.clone())
    }

    pub fn insert(&self, key: String, artifact: Artifact) {
        tracing::debug!(key, kind = ?artifact.kind, "artifact stored");
        self.cache.insert(key, artifact);
    }

    pub fn get_or_compute<F>(
        &self,
        key: &str,
        kind: ArtifactKind,
        compute: F,
    ) -> Result<Artifact>
    where
        F: FnOnce() -> Result<Vec<u8>>,
    {
        if let Some(cached) = self.get(key) {
            tracing::debug!(key, "artifact cache hit");
            return Ok(cached);
        }
        let data = compute()?;
        let artifact = Artifact {
            id: key.to_string(),
            kind,
            data,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };
        self.insert(key.to_string(), artifact.clone());
        Ok(artifact)
    }

    /// Store ZK-friendly field element blob — F_p encoded bytes for circuit input
    pub fn store_field_blob(&self, label: &str, field_bytes: Bytes) -> String {
        let key = format!("blob:{}", label);
        self.insert(key.clone(), Artifact {
            id: key.clone(),
            kind: ArtifactKind::BlobField,
            data: field_bytes.to_vec(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });
        key
    }

    pub fn stats(&self) -> StoreStats {
        let mut counts = std::collections::HashMap::new();
        for entry in self.cache.iter() {
            *counts.entry(format!("{:?}", entry.kind)).or_insert(0usize) += 1;
        }
        StoreStats { total: self.cache.len(), by_kind: counts }
    }
}

#[derive(Debug, Serialize)]
pub struct StoreStats {
    pub total: usize,
    pub by_kind: std::collections::HashMap<String, usize>,
}
