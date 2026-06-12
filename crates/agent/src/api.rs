use anyhow::Result;
use serde_json::json;
use tracing::info;

pub struct Agent {
    api_key: String,
    client: reqwest::Client,
}

impl Agent {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(&self, prompt: &str) -> Result<String> {
        info!("sending prompt to Claude: {}", prompt);

        let body = json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 2048,
            "system": "You are axiom-engine — a proof-first AI. Always verify claims mathematically before answering.",
            "messages": [{ "role": "user", "content": prompt }]
        });

        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let text = resp["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        info!("response: {}", &text[..text.len().min(200)]);
        Ok(text)
    }
}
