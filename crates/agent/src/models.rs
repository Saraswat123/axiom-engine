/// Open-source model integration — axiom-engine is model-agnostic.
/// Swap providers by setting AXIOM_MODEL env var.

use anyhow::Result;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum ModelProvider {
    Anthropic { model: String },      // claude-sonnet-4-6 (default)
    Ollama { model: String, url: String }, // local llama3/mistral/phi
    OpenAI { model: String },         // gpt-4o (fallback)
    HuggingFace { model: String },    // via inference API
}

impl ModelProvider {
    pub fn from_env() -> Self {
        let provider = std::env::var("AXIOM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
        let model = std::env::var("AXIOM_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        match provider.as_str() {
            "ollama" => Self::Ollama {
                model,
                url: std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()),
            },
            "openai" => Self::OpenAI { model },
            "huggingface" => Self::HuggingFace { model },
            _ => Self::Anthropic { model },
        }
    }

    pub async fn complete(&self, client: &reqwest::Client, system: &str, prompt: &str) -> Result<String> {
        match self {
            Self::Anthropic { model } => anthropic_complete(client, model, system, prompt).await,
            Self::Ollama { model, url } => ollama_complete(client, model, url, system, prompt).await,
            Self::OpenAI { model } => openai_complete(client, model, system, prompt).await,
            Self::HuggingFace { model } => hf_complete(client, model, system, prompt).await,
        }
    }
}

async fn anthropic_complete(client: &reqwest::Client, model: &str, system: &str, prompt: &str) -> Result<String> {
    let key = std::env::var("ANTHROPIC_API_KEY")?;
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": model,
            "max_tokens": 2048,
            "system": system,
            "messages": [{ "role": "user", "content": prompt }]
        }))
        .send().await?
        .json::<Value>().await?;
    Ok(resp["content"][0]["text"].as_str().unwrap_or("").to_string())
}

async fn ollama_complete(client: &reqwest::Client, model: &str, url: &str, system: &str, prompt: &str) -> Result<String> {
    let resp = client
        .post(format!("{}/api/chat", url))
        .json(&json!({
            "model": model,
            "stream": false,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": prompt }
            ]
        }))
        .send().await?
        .json::<Value>().await?;
    Ok(resp["message"]["content"].as_str().unwrap_or("").to_string())
}

async fn openai_complete(client: &reqwest::Client, model: &str, system: &str, prompt: &str) -> Result<String> {
    let key = std::env::var("OPENAI_API_KEY")?;
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": prompt }
            ]
        }))
        .send().await?
        .json::<Value>().await?;
    Ok(resp["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
}

async fn hf_complete(client: &reqwest::Client, model: &str, _system: &str, prompt: &str) -> Result<String> {
    let key = std::env::var("HF_API_KEY")?;
    let resp = client
        .post(format!("https://api-inference.huggingface.co/models/{}", model))
        .bearer_auth(key)
        .json(&json!({ "inputs": prompt }))
        .send().await?
        .json::<Value>().await?;
    Ok(resp[0]["generated_text"].as_str().unwrap_or("").to_string())
}
