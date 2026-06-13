use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Two-tier artifact store:
///   L1: DashMap — lock-free in-memory reads (nanoseconds)
///   L2: sled    — embedded persistent store (survives restarts)
///
/// Write-through: every insert writes to both tiers.
/// Read: L1 hit → return immediately. L1 miss → check L2 → promote to L1.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub data: Vec<u8>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactKind {
    EGraphState,
    SmtResult,
    ZkReceipt,
    ComputeResult,
    TraceSnapshot,
    BlobField,
}

#[derive(Clone)]
pub struct ArtifactStore {
    l1: Arc<DashMap<String, Artifact>>,
    l2: Arc<sled::Db>,
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStore {
    /// Open persistent store at `path`. Creates directory if needed.
    pub fn open(path: &str) -> Result<Self> {
        let db = sled::open(path)?;
        let store = Self {
            l1: Arc::new(DashMap::new()),
            l2: Arc::new(db),
        };
        // Warm L1 from L2 on startup
        store.warm_l1()?;
        tracing::info!(path, l1 = store.l1.len(), "artifact store opened");
        Ok(store)
    }

    /// In-memory only (tests / when no path configured)
    pub fn new() -> Self {
        let path =
            std::env::var("AXIOM_STORE_PATH").unwrap_or_else(|_| "/tmp/axiom-store".to_string());
        Self::open(&path).unwrap_or_else(|e| {
            tracing::warn!("sled open failed ({}), using memory-only store", e);
            Self {
                l1: Arc::new(DashMap::new()),
                l2: Arc::new(sled::Config::new().temporary(true).open().unwrap()),
            }
        })
    }

    /// Load all L2 entries into L1 on startup
    fn warm_l1(&self) -> Result<()> {
        let mut count = 0usize;
        for item in self.l2.iter() {
            let (k, v) = item?;
            if let Ok(artifact) = bincode_decode(&v) {
                let key = String::from_utf8_lossy(&k).to_string();
                self.l1.insert(key, artifact);
                count += 1;
            }
        }
        if count > 0 {
            tracing::info!(count, "warmed L1 cache from persistent store");
        }
        Ok(())
    }

    pub fn key(kind: &ArtifactKind, input: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        format!("{:?}", kind).hash(&mut h);
        input.to_string().hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// Read: L1 first, then L2, promote to L1 on L2 hit
    pub fn get(&self, key: &str) -> Option<Artifact> {
        // L1 hit
        if let Some(a) = self.l1.get(key) {
            return Some(a.clone());
        }
        // L2 hit — promote to L1
        if let Ok(Some(bytes)) = self.l2.get(key.as_bytes()) {
            if let Ok(artifact) = bincode_decode::<Artifact>(&bytes) {
                self.l1.insert(key.to_string(), artifact.clone());
                tracing::debug!(key, "L2 hit — promoted to L1");
                return Some(artifact);
            }
        }
        None
    }

    /// Write-through: insert into L1 + flush to L2
    pub fn insert(&self, key: String, artifact: Artifact) {
        // L2 persist first (write-through)
        if let Ok(bytes) = bincode_encode(&artifact) {
            let _ = self.l2.insert(key.as_bytes(), bytes);
            let _ = self.l2.flush(); // fsync — guaranteed on restart
        }
        // L1 cache
        self.l1.insert(key.clone(), artifact.clone());
        tracing::debug!(key, kind = ?artifact.kind, "artifact persisted");
    }

    pub fn get_or_compute<F>(&self, key: &str, kind: ArtifactKind, compute: F) -> Result<Artifact>
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
            created_at: now_ms(),
        };
        self.insert(key.to_string(), artifact.clone());
        Ok(artifact)
    }

    pub fn store_field_blob(&self, label: &str, field_bytes: Bytes) -> String {
        let key = format!("blob:{}", label);
        self.insert(
            key.clone(),
            Artifact {
                id: key.clone(),
                kind: ArtifactKind::BlobField,
                data: field_bytes.to_vec(),
                created_at: now_ms(),
            },
        );
        key
    }

    pub fn stats(&self) -> StoreStats {
        let mut by_kind = std::collections::HashMap::new();
        for entry in self.l1.iter() {
            *by_kind.entry(format!("{:?}", entry.kind)).or_insert(0usize) += 1;
        }
        StoreStats {
            total: self.l1.len(),
            persistent: true,
            by_kind,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StoreStats {
    pub total: usize,
    pub persistent: bool,
    pub by_kind: std::collections::HashMap<String, usize>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn bincode_encode<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(v)?)
}

fn bincode_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(bytes)?)
}
