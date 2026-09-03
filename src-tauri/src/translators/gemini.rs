use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn check_gemini_multimodal(body: &Value) -> bool {
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if let Some(content) = msg.get("content") {
                if let Some(arr) = content.as_array() {
                    for item in arr {
                        if item.get("type").and_then(|t| t.as_str()) == Some("image_url") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

pub fn translate_tools_to_gemini(tools: &Value) -> Option<Value> {
    if let Some(arr) = tools.as_array() {
        let mut declarations = Vec::new();
        for t in arr {
            if t.get("type").and_then(|v| v.as_str()) == Some("function") {
                if let Some(function) = t.get("function") {
                    let mut decl = json!({
                        "name": function.get("name").unwrap_or(&json!("")),
                        "description": function.get("description").unwrap_or(&json!("")),
                    });
                    if let Some(params) = function.get("parameters") {
                        decl.as_object_mut().unwrap().insert("parameters".to_string(), params.clone());
                    }
                    declarations.push(decl);
                }
            }
        }
        if !declarations.is_empty() {
            return Some(json!([{
                "functionDeclarations": declarations
            }]));
        }
    }
    None
}

pub fn build_gemini_payload(body: &Value) -> (Value, bool) {
    let mut gemini_payload = json!({});
    let is_stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    // Translate messages to contents & systemInstruction
    let mut contents = Vec::new();
    let mut system_text: Option<String> = None;

    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for m in messages {
            let role_str = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content_str = m.get("content").and_then(|c| c.as_str()).unwrap_or("");

            if role_str == "system" {
                system_text = Some(content_str.to_string());
                continue;
            }

            let role = match role_str {
                "assistant" => "model",
                _ => "user",
            };

            contents.push(json!({
                "role": role,
                "parts": [{ "text": content_str }]
            }));
        }
    }
    gemini_payload["contents"] = json!(contents);

    if let Some(sys) = system_text {
        gemini_payload["systemInstruction"] = json!({
            "parts": [{ "text": sys }]
        });
    }

    // Translate tools if present
    if let Some(tools) = body.get("tools") {
        if let Some(gemini_tools) = translate_tools_to_gemini(tools) {
            gemini_payload["tools"] = gemini_tools;
        }
    }

    // Translate generation config options
    let mut gen_config = json!({});
    if let Some(temp) = body.get("temperature").and_then(|t| t.as_f64()) {
        gen_config["temperature"] = json!(temp);
    }
    if let Some(top_p) = body.get("top_p").and_then(|t| t.as_f64()) {
        gen_config["topP"] = json!(top_p);
    }
    if let Some(max_tokens) = body.get("max_tokens").and_then(|m| m.as_u64()) {
        gen_config["maxOutputTokens"] = json!(max_tokens);
    }
    if let Some(stop) = body.get("stop") {
        if stop.is_array() {
            gen_config["stopSequences"] = stop.clone();
        } else if let Some(s) = stop.as_str() {
            gen_config["stopSequences"] = json!([s]);
        }
    }

    if gen_config.as_object().map_or(false, |o| !o.is_empty()) {
        gemini_payload["generationConfig"] = gen_config;
    }

    (gemini_payload, is_stream)
}

pub fn transform_gemini_response_to_openai(gemini_json: &Value, model: &str) -> Value {
    let id = format!("chatcmpl-{}", Uuid::new_v4().simple());
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut message = json!({
        "role": "assistant"
    });

    let mut finish_reason = "stop";

    if let Some(candidate) = gemini_json.get("candidates").and_then(|c| c.get(0)) {
        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
            match reason {
                "MAX_TOKENS" => finish_reason = "length",
                "SAFETY" | "RECITATION" => finish_reason = "content_filter",
                _ => {}
            }
        }

        if let Some(content) = candidate.get("content") {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                for part in parts {
                    if let Some(t) = part.get("text").and_then(|s| s.as_str()) {
                        text_parts.push(t);
                    }
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let args = fc.get("args").cloned().unwrap_or(json!({}));
                        let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());

                        tool_calls.push(json!({
                            "id": format!("call_{}", Uuid::new_v4().simple()),
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": args_str
                            }
                        }));
                    }
                }
            }

            if !text_parts.is_empty() {
                message["content"] = json!(text_parts.join(""));
            } else if tool_calls.is_empty() {
                message["content"] = json!("");
            } else {
                message["content"] = Value::Null;
            }

            if !tool_calls.is_empty() {
                message["tool_calls"] = json!(tool_calls);
                finish_reason = "tool_calls";
            }
        }
    }

    let mut usage = json!({
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "total_tokens": 0
    });

    if let Some(um) = gemini_json.get("usageMetadata") {
        let pt = um.get("promptTokenCount").and_then(|n| n.as_u64()).unwrap_or(0);
        let ct = um.get("candidatesTokenCount").and_then(|n| n.as_u64()).unwrap_or(0);
        let tt = um.get("totalTokenCount").and_then(|n| n.as_u64()).unwrap_or(pt + ct);
        usage = json!({
            "prompt_tokens": pt,
            "completion_tokens": ct,
            "total_tokens": tt
        });
    }

    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": format!("gemini/{}", model),
        "choices": [
            {
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }
        ],
        "usage": usage
    })
}

pub fn transform_gemini_chunk_to_openai(chunk_json: &Value, model: &str, chunk_id: &str) -> Option<Value> {
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let candidate = chunk_json.get("candidates")?.get(0)?;
    let mut delta = json!({});
    let mut finish_reason = Value::Null;

    if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
        match reason {
            "STOP" => finish_reason = json!("stop"),
            "MAX_TOKENS" => finish_reason = json!("length"),
            "SAFETY" | "RECITATION" => finish_reason = json!("content_filter"),
            _ => finish_reason = json!("stop"),
        }
    }

    if let Some(content) = candidate.get("content") {
        if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for (idx, part) in parts.iter().enumerate() {
                if let Some(t) = part.get("text").and_then(|s| s.as_str()) {
                    text_parts.push(t);
                }
                if let Some(fc) = part.get("functionCall") {
                    let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args = fc.get("args").cloned().unwrap_or(json!({}));
                    let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());

                    tool_calls.push(json!({
                        "index": idx,
                        "id": format!("call_{}", Uuid::new_v4().simple()),
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": args_str
                        }
                    }));
                }
            }

            if !text_parts.is_empty() {
                delta["content"] = json!(text_parts.join(""));
            }
            if !tool_calls.is_empty() {
                delta["tool_calls"] = json!(tool_calls);
                finish_reason = json!("tool_calls");
            }
        }
    }

    Some(json!({
        "id": chunk_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": format!("gemini/{}", model),
        "choices": [
            {
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason
            }
        ]
    }))
}

pub async fn forward_gemini_request(
    client: &Client,
    api_key: &str,
    target_model: &str,
    body: Value,
) -> Result<reqwest::Response, String> {
    if check_gemini_multimodal(&body) {
        return Err("multimodal input requires inlineData conversion, not yet supported for Gemini (gemini_multimodal_unsupported)".to_string());
    }

    let (gemini_payload, is_stream) = build_gemini_payload(&body);

    let action = if is_stream { "streamGenerateContent?alt=sse&" } else { "generateContent?" };
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:{}key={}",
        target_model, action, api_key
    );

    let res = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&gemini_payload)
        .send()
        .await
        .map_err(|e| format!("Gemini upstream error: {}", e))?;

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_response_transformation() {
        let gemini_raw = json!({
            "candidates": [
                {
                    "content": {
                        "parts": [{ "text": "Hello from Gemini!" }],
                        "role": "model"
                    },
                    "finishReason": "STOP"
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 10,
                "totalTokenCount": 15
            }
        });

        let openai_shaped = transform_gemini_response_to_openai(&gemini_raw, "gemini-1.5-pro");
        assert_eq!(openai_shaped["object"], "chat.completion");
        assert_eq!(openai_shaped["model"], "gemini/gemini-1.5-pro");
        assert_eq!(openai_shaped["choices"][0]["message"]["content"], "Hello from Gemini!");
        assert_eq!(openai_shaped["choices"][0]["finish_reason"], "stop");
        assert_eq!(openai_shaped["usage"]["total_tokens"], 15);
    }

    #[test]
    fn test_tool_call_response_transformation() {
        let gemini_raw = json!({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {
                                "functionCall": {
                                    "name": "get_weather",
                                    "args": { "city": "London" }
                                }
                            }
                        ],
                        "role": "model"
                    },
                    "finishReason": "STOP"
                }
            ]
        });

        let openai_shaped = transform_gemini_response_to_openai(&gemini_raw, "gemini-1.5-flash");
        let tool_calls = openai_shaped["choices"][0]["message"]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(openai_shaped["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn test_streaming_chunk_transformation() {
        let chunk_raw = json!({
            "candidates": [
                {
                    "content": {
                        "parts": [{ "text": "streaming text" }]
                    }
                }
            ]
        });

        let chunk = transform_gemini_chunk_to_openai(&chunk_raw, "gemini-1.5-pro", "test-chunk-id").unwrap();
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert_eq!(chunk["choices"][0]["delta"]["content"], "streaming text");
    }
}
