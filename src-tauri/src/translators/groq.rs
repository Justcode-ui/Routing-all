use reqwest::Client;
use serde_json::Value;

pub async fn forward_groq_request(
    client: &Client,
    api_key: &str,
    target_model: &str,
    mut body: Value,
) -> Result<reqwest::Response, String> {
    // Replace model field with actual target model name
    if let Some(obj) = body.as_object_mut() {
        obj.insert("model".to_string(), Value::String(target_model.to_string()));
    }

    let res = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Groq upstream error: {}", e))?;

    Ok(res)
}
