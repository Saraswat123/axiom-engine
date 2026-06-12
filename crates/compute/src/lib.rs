use anyhow::Result;
use nalgebra::DMatrix;
use rayon::prelude::*;
use serde_json::{json, Value};

/// Field element blob — F_p arithmetic aligned for ZK circuits.
/// Uses ark-ff BN254 scalar field (same field as Ethereum's BN254 precompile).
pub mod field {
    use ark_ff::{Field, PrimeField};
    use ark_bn254::Fr; // BN254 scalar field — 254-bit prime

    /// Encode a matrix row as a vector of field elements.
    /// Each f64 → nearest integer → Fr field element.
    pub fn matrix_row_to_field(row: &[f64]) -> Vec<Fr> {
        row.iter()
            .map(|&v| Fr::from(v.round() as i64 as u64))
            .collect()
    }

    /// Inner product over F_p — ZK circuit friendly
    pub fn field_inner_product(a: &[Fr], b: &[Fr]) -> Fr {
        a.iter().zip(b.iter()).map(|(x, y)| *x * y).sum()
    }

    /// Serialize field elements to bytes for store/ZK circuit input
    pub fn to_blob(elements: &[Fr]) -> Vec<u8> {
        elements.iter()
            .flat_map(|e| {
                use ark_ff::BigInteger;
                e.into_bigint().to_bytes_le()
            })
            .collect()
    }
}

pub struct ComputeTool;

impl ComputeTool {
    pub fn new() -> Self { Self }

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
                let sym = matrix.clone().symmetric_eigen();
                let vals: Vec<f64> = sym.eigenvalues.iter().cloned().collect();
                json!({ "eigenvalues": vals })
            }
            // ZK-aligned: convert matrix to field element blob
            "field_blob" => {
                let rows: Vec<Vec<ark_bn254::Fr>> = (0..n)
                    .map(|i| field::matrix_row_to_field(&data[i*n..(i+1)*n]))
                    .collect();
                let blob: Vec<u8> = rows.iter()
                    .flat_map(|r| field::to_blob(r))
                    .collect();
                json!({
                    "field": "BN254",
                    "blob_len": blob.len(),
                    "blob_hex": hex::encode(&blob[..blob.len().min(64)])
                })
            }
            _ => anyhow::bail!("unknown op: {}", op),
        };

        Ok(result)
    }

    /// Parallel batch via rayon work-stealing — all CPU cores
    pub fn batch_determinants(&self, matrices: Vec<Vec<f64>>) -> Vec<f64> {
        matrices.par_iter()
            .map(|data| {
                let n = (data.len() as f64).sqrt() as usize;
                DMatrix::from_row_slice(n, n, data).determinant()
            })
            .collect()
    }

    /// Parallel field blob generation — for ZK circuit batch input
    pub fn batch_field_blobs(&self, matrices: Vec<Vec<f64>>) -> Vec<Vec<u8>> {
        matrices.par_iter()
            .map(|data| {
                let n = (data.len() as f64).sqrt() as usize;
                (0..n)
                    .flat_map(|i| field::to_blob(&field::matrix_row_to_field(&data[i*n..(i+1)*n])))
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_det() {
        let tool = ComputeTool::new();
        let r = tool.batch_determinants(vec![vec![1.0, 0.0, 0.0, 1.0]]);
        assert!((r[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_field_encoding() {
        let row = vec![1.0, 2.0, 3.0];
        let elems = field::matrix_row_to_field(&row);
        assert_eq!(elems.len(), 3);
        let blob = field::to_blob(&elems);
        assert!(!blob.is_empty());
    }
}
