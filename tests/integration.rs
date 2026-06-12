/// Integration tests — run against local axiom-engine HTTP server.
/// Start server first: cargo run -p axiom-mcp-server (with AXIOM_TRANSPORT=http)
///
/// Run: cargo test --test integration -- --test-threads=1

#[cfg(test)]
mod integration {
    const BASE: &str = "http://localhost:8080";

    #[tokio::test]
    #[ignore] // requires running server: AXIOM_TRANSPORT=http cargo run -p axiom-mcp-server
    async fn test_health() {
        let resp = reqwest::get(format!("{}/health", BASE))
            .await.unwrap()
            .json::<serde_json::Value>().await.unwrap();
        assert_eq!(resp["status"], "ok");
    }

    #[tokio::test]
    #[ignore]
    async fn test_z3_prove_via_http() {
        let client = reqwest::Client::new();
        let resp = client.post(format!("{}/tools", BASE))
            .json(&serde_json::json!({
                "tool": "z3_prove",
                "input": { "property": "square_positive", "low": 1, "high": 100 }
            }))
            .send().await.unwrap()
            .json::<serde_json::Value>().await.unwrap();

        assert_eq!(resp["ok"], true);
        assert_eq!(resp["result"]["verdict"], "proved");
    }

    #[tokio::test]
    #[ignore]
    async fn test_opt_verify_pipeline() {
        let client = reqwest::Client::new();
        let resp = client.post(format!("{}/tools", BASE))
            .json(&serde_json::json!({
                "tool": "opt_verify",
                "input": { "expression": "(+ x 0)" }
            }))
            .send().await.unwrap()
            .json::<serde_json::Value>().await.unwrap();

        assert_eq!(resp["ok"], true);
        let result = &resp["result"];
        assert_eq!(result["optimized"], "x");
        assert_eq!(result["z3_verdict"], "proved");
    }

    #[tokio::test]
    #[ignore]
    async fn test_cache_hit_on_repeat() {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "tool": "opt_verify",
            "input": { "expression": "(* x 1)" }
        });

        // First call — no cache
        let r1 = client.post(format!("{}/tools", BASE))
            .json(&body).send().await.unwrap()
            .json::<serde_json::Value>().await.unwrap();
        assert_eq!(r1["result"]["cache_hit"], false);

        // Second call — cache hit
        let r2 = client.post(format!("{}/tools", BASE))
            .json(&body).send().await.unwrap()
            .json::<serde_json::Value>().await.unwrap();
        assert_eq!(r2["result"]["cache_hit"], true);
    }
}
