#![no_main]
risc0_zkvm::guest::entry!(main);

use risc0_zkvm::guest::env;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ComputeInput {
    pub op: String,
    pub data: Vec<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct ComputeOutput {
    pub op: String,
    pub result: f64,
    pub input_hash: u64,
}

fn main() {
    // Read private input (not revealed in proof)
    let input: ComputeInput = env::read();

    // Compute inside zkVM — every instruction recorded in execution trace
    let result = match input.op.as_str() {
        "sum"  => input.data.iter().sum(),
        "dot"  => {
            let n = input.data.len() / 2;
            input.data[..n].iter().zip(input.data[n..].iter())
                .map(|(a, b)| a * b).sum()
        }
        "norm" => input.data.iter().map(|x| x * x).sum::<f64>().sqrt(),
        _      => 0.0,
    };

    // Hash of input (deterministic, for commitment)
    let input_hash = input.data.iter()
        .fold(0u64, |acc, &x| acc.wrapping_add(x.to_bits()));

    // Commit public output to journal
    env::commit(&ComputeOutput {
        op: input.op,
        result,
        input_hash,
    });
}
