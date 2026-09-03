#[derive(Debug, PartialEq, Eq)]
pub enum Provider {
    Groq,
    Gemini,
    OpenRouter,
}

#[derive(Debug)]
pub struct ParsedRoute {
    pub provider: Provider,
    pub target_model: String,
}

pub fn parse_model_route(raw_model: &str) -> Result<ParsedRoute, String> {
    // Split ONLY on the first slash (PRD FR-4)
    let parts: Vec<&str> = raw_model.splitn(2, '/').collect();
    if parts.len() < 2 {
        return Err("Model string missing provider prefix. Expected syntax: '<provider>/<model>'".to_string());
    }

    let provider_str = parts[0].to_lowercase();
    let target_model = parts[1].to_string();

    let provider = match provider_str.as_str() {
        "groq" => Provider::Groq,
        "gemini" | "google" => Provider::Gemini,
        "openrouter" => Provider::OpenRouter,
        _ => return Err(format!("Unrecognized provider prefix: '{}'", provider_str)),
    };

    Ok(ParsedRoute {
        provider,
        target_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_slash_split() {
        let route = parse_model_route("openrouter/anthropic/claude-3-haiku").unwrap();
        assert_eq!(route.provider, Provider::OpenRouter);
        assert_eq!(route.target_model, "anthropic/claude-3-haiku");
    }

    #[test]
    fn test_groq_route() {
        let route = parse_model_route("groq/llama-3.1-70b-versatile").unwrap();
        assert_eq!(route.provider, Provider::Groq);
        assert_eq!(route.target_model, "llama-3.1-70b-versatile");
    }
}
