use anyhow::Result;
use serde_json::{json, Value};
use nalgebra::DMatrix;
use rayon::prelude::*;

pub struct ComputeTool;

impl ComputeTool {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(&self, input: Value) -> Result<Value> {
        let op = input["op"].as_str().unwrap_or("determinant");
        let data: Vec<f64> = serde_json::from_value(input["data"].clone())?;
        let n = (data.len() as f64).sqrt() as usize;

        anyhow::ensure!(n * n == data.len(), "data must be square matrix");

        let matrix = DMatrix::from_row_slice(n, n, &data);

        let result = match op {
            "determinant" => json!({ "determinant": matrix.determinant() }),
            "trace" => json!({ "trace": matrix.trace() }),
            "norm" => json!({ "norm": matrix.norm() }),
            "eigenvalues" => {
                // Symmetric eigendecomposition
                let sym = matrix.symmetric_eigen();
                let vals: Vec<f64> = sym.eigenvalues.iter().cloned().collect();
                json!({ "eigenvalues": vals })
            }
            _ => anyhow::bail!("unknown op: {}", op),
        };

        Ok(result)
    }

    // Parallel batch — all cores
    pub fn batch_determinants(&self, matrices: Vec<Vec<f64>>) -> Vec<f64> {
        matrices.par_iter()
            .map(|data| {
                let n = (data.len() as f64).sqrt() as usize;
                let m = DMatrix::from_row_slice(n, n, data);
                m.determinant()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_determinant() {
        let tool = ComputeTool::new();
        // 2x2 identity matrix det = 1
        let data = vec![1.0, 0.0, 0.0, 1.0];
        let matrices = vec![data];
        let results = tool.batch_determinants(matrices);
        assert!((results[0] - 1.0).abs() < 1e-10);
    }
}
