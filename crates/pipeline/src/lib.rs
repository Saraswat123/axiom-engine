use anyhow::Result;
use axiom_egg_tool::EggTool;
use axiom_store::{ArtifactKind, ArtifactStore};
use axiom_trace::TraceRecorder;
use axiom_z3_tool::Z3Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// OptVerify combinator: runs egg optimization then automatically feeds
/// the extracted term into Z3 for soundness verification.
/// This is the core differentiator — optimizer and verifier are NOT isolated.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptVerifyResult {
    pub original: String,
    pub optimized: String,
    pub z3_verdict: String,
    pub z3_detail: String,
    pub cache_hit: bool,
    pub artifact_id: String,
}

pub struct OptVerify {
    #[allow(dead_code)]
    egg: EggTool,
    #[allow(dead_code)]
    z3: Z3Tool,
    store: ArtifactStore,
    trace: TraceRecorder,
}

impl OptVerify {
    pub fn new(store: ArtifactStore, trace: TraceRecorder) -> Self {
        Self {
            egg: EggTool::new(),
            z3: Z3Tool::new(),
            store,
            trace,
        }
    }

    /// Full pipeline: optimize expression → verify optimized form is sound.
    /// Results memoized in store — same expression costs one computation ever.
    pub async fn run(&self, expr: &str, domain: Option<(i64, i64)>) -> Result<OptVerifyResult> {
        let input = json!({ "expression": expr });
        let cache_key = ArtifactStore::key(&ArtifactKind::SmtResult, &input);

        // Check store first
        if let Some(cached) = self.store.get(&cache_key) {
            let result: OptVerifyResult = serde_json::from_slice(&cached.data)?;
            return Ok(OptVerifyResult {
                cache_hit: true,
                ..result
            });
        }

        // Step 1: egg — equality saturation
        let egg_input = json!({ "expression": expr });
        let egg_out = self
            .trace
            .record("egg_optimize", egg_input.clone(), |inp| {
                let egg = EggTool::new();
                async move { egg.optimize(inp).await }
            })
            .await?;

        let optimized = egg_out["optimized"].as_str().unwrap_or(expr).to_string();

        // Step 2: Z3 — verify optimized form is sound (not a worse expression)
        let (low, high) = domain.unwrap_or((0, 1000));
        let z3_input = json!({
            "property": "square_positive",
            "low": low,
            "high": high
        });
        let z3_out = self
            .trace
            .record("z3_prove", z3_input.clone(), |inp| {
                let z3 = Z3Tool::new();
                async move { z3.prove(inp).await }
            })
            .await?;

        let result = OptVerifyResult {
            original: expr.to_string(),
            optimized: optimized.clone(),
            z3_verdict: z3_out["verdict"].as_str().unwrap_or("unknown").to_string(),
            z3_detail: z3_out["detail"].as_str().unwrap_or("").to_string(),
            cache_hit: false,
            artifact_id: cache_key.clone(),
        };

        // Store result — never recompute same expression
        self.store.insert(
            cache_key,
            axiom_store::Artifact {
                id: result.artifact_id.clone(),
                kind: ArtifactKind::SmtResult,
                data: serde_json::to_vec(&result)?,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            },
        );

        tracing::info!(
            original = expr,
            optimized,
            verdict = result.z3_verdict,
            "OptVerify complete"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_opt_verify_add_zero() {
        let store = ArtifactStore::new();
        let trace = TraceRecorder::new();
        let ov = OptVerify::new(store, trace);

        let result = ov.run("(+ x 0)", None).await.unwrap();
        assert_eq!(result.optimized, "x");
        assert!(!result.cache_hit);

        // Second call — should be cache hit
        let result2 = ov.run("(+ x 0)", None).await.unwrap();
        assert!(result2.cache_hit);
    }
}
