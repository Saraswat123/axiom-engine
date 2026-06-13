use anyhow::Result;
use serde_json::{json, Value};
use z3::{ast::Int, Config, Context, SatResult, Solver};

#[derive(Default)]
pub struct Z3Tool {
    // Z3 Context is not Send — create per call
}

impl Z3Tool {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn prove(&self, input: Value) -> Result<Value> {
        let low = input["low"].as_i64().unwrap_or(0);
        let high = input["high"].as_i64().unwrap_or(1000);
        let property = input["property"].as_str().unwrap_or("positive");

        let result = self.verify_bounded(low, high, property);
        Ok(json!({ "verdict": result.0, "detail": result.1 }))
    }

    fn verify_bounded(&self, low: i64, high: i64, property: &str) -> (String, String) {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);

        let x = Int::new_const(&ctx, "x");
        let lo = Int::from_i64(&ctx, low);
        let hi = Int::from_i64(&ctx, high);

        // Assert domain: low <= x <= high
        solver.assert(&x.ge(&lo));
        solver.assert(&x.le(&hi));

        // Property: x > 0 → x*x > 0
        match property {
            "square_positive" => {
                use std::ops::Mul;
                let zero = Int::from_i64(&ctx, 0);
                let x_sq = x.clone().mul(&x);
                // Assert negation: x > 0 AND x*x <= 0
                solver.assert(&x.gt(&zero));
                solver.assert(&x_sq.le(&zero));
            }
            _ => {
                return (
                    "unknown_property".to_string(),
                    format!("property '{}' not implemented", property),
                );
            }
        }

        match solver.check() {
            SatResult::Unsat => (
                "proved".to_string(),
                "property holds for all values in range".to_string(),
            ),
            SatResult::Sat => (
                "counterexample".to_string(),
                "property violated — model found".to_string(),
            ),
            SatResult::Unknown => ("unknown".to_string(), "solver timed out".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_positive() {
        let tool = Z3Tool::new();
        let (verdict, _) = tool.verify_bounded(1, 1000, "square_positive");
        assert_eq!(verdict, "proved");
    }
}
