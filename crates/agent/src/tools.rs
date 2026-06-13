// Tool dispatch — each tool is a separate crate
// Wired together here for the agent loop

use anyhow::Result;
use axiom_compute::ComputeTool;
use axiom_egg_tool::EggTool;
use axiom_z3_tool::Z3Tool;

pub struct ToolRegistry {
    pub z3: Z3Tool,
    pub egg: EggTool,
    pub compute: ComputeTool,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            z3: Z3Tool::new(),
            egg: EggTool::new(),
            compute: ComputeTool::new(),
        }
    }

    pub async fn dispatch(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        match name {
            "z3_prove" => self.z3.prove(input).await,
            "egg_optimize" => self.egg.optimize(input).await,
            "compute_matrix" => self.compute.run(input).await,
            _ => anyhow::bail!("unknown tool: {}", name),
        }
    }
}
