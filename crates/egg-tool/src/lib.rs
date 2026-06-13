use anyhow::Result;
use egg::{rewrite, AstSize, Extractor, RecExpr, Rewrite, Runner, SymbolLang};
use serde_json::{json, Value};

pub struct EggTool;

impl Default for EggTool {
    fn default() -> Self {
        Self
    }
}

impl EggTool {
    pub fn new() -> Self {
        Self
    }

    pub async fn optimize(&self, input: Value) -> Result<Value> {
        let expr = input["expression"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'expression' field"))?;

        let optimized = self.run_equality_saturation(expr)?;
        Ok(json!({
            "original": expr,
            "optimized": optimized
        }))
    }

    fn run_equality_saturation(&self, expr: &str) -> Result<String> {
        let rules: Vec<Rewrite<SymbolLang, ()>> = vec![
            rewrite!("add-zero";  "(+ ?x 0)" => "?x"),
            rewrite!("mul-one";   "(* ?x 1)" => "?x"),
            rewrite!("mul-zero";  "(* ?x 0)" => "0"),
            rewrite!("add-comm";  "(+ ?x ?y)" => "(+ ?y ?x)"),
            rewrite!("mul-comm";  "(* ?x ?y)" => "(* ?y ?x)"),
        ];

        let parsed: RecExpr<SymbolLang> = expr
            .parse()
            .map_err(|e| anyhow::anyhow!("parse error: {}", e))?;

        let runner = Runner::default().with_expr(&parsed).run(&rules);

        let extractor = Extractor::new(&runner.egraph, AstSize);
        let (_, best) = extractor.find_best(runner.roots[0]);
        Ok(best.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_add_zero() {
        let tool = EggTool::new();
        let result = tool.run_equality_saturation("(+ x 0)").unwrap();
        assert_eq!(result, "x");
    }

    #[test]
    fn test_simplify_mul_one() {
        let tool = EggTool::new();
        let result = tool.run_equality_saturation("(* x 1)").unwrap();
        assert_eq!(result, "x");
    }
}
