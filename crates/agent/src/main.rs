use anyhow::Result;
use tracing::info;

mod api;
mod tools;
mod types;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("axiom-engine agent starting");

    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

    let agent = api::Agent::new(api_key);
    agent.run("Prove that for all x > 0, x^2 > 0").await?;

    Ok(())
}
