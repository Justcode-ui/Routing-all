use reqwest::Client;
use serde_json::Value;

pub async fn forward_openrouter_request(
    client: &Client,
    api_key: &str,
    target_model: &str,
    mut body: Value,
) -> Result<reqwest::Response, String> {
    if let Some(obj) = body.as_object_mut() {
        obj.insert("model".to_string(), Value::String(target_model.to_string()));
    }

    let res = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenRouter upstream error: {}", e))?;

    Ok(res)
}
