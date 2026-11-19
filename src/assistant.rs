use crate::credentials::{self, Provider};
use curl::easy::{Easy, List};
use serde_json::{Value, json};
use std::time::Duration;

const INSTRUCTIONS: &str = "Help the user work in a macOS terminal. Give concise plain-text answers. When suggesting a command, explain what it does. You have no access to their terminal.";
const MAX_QUESTION_CHARS: usize = 8_000;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_TOKENS: u32 = 2048;

pub fn ask(provider: Provider, question: &str) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Enter a question first.".into());
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err("Use 8,000 characters or fewer.".into());
    }
    let key = credentials::lookup(provider)?;
    let (url, model_env, default_model) = match provider {
        Provider::OpenAi => (
            "https://api.openai.com/v1/responses",
            "MINTERM_AI_MODEL",
            "gpt-5.4-mini",
        ),
        Provider::Anthropic => (
            "https://api.anthropic.com/v1/messages",
            "MINTERM_ANTHROPIC_MODEL",
            "claude-haiku-5-5",
        ),
    };
    let model = std::env::var(model_env)
        .ok()
        .filter(|s| !s.trim().is_empty());
    let body = request_body(
        provider,
        question,
        model.as_deref().unwrap_or(default_model),
    );

    let mut request = Easy::new();
    let mut headers = List::new();
    headers
        .append("Content-Type: application/json")
        .map_err(network_error)?;
    headers
        .append(&format!("Authorization: Bearer {key}"))
        .map_err(network_error)?;
    if provider == Provider::Anthropic {
        headers
            .append("anthropic-version: 2026-10-21")
            .map_err(network_error)?;
    }
    request.url(url).map_err(network_error)?;
    request.http_headers(headers).map_err(network_error)?;
    request
        .connect_timeout(Duration::from_secs(10))
        .map_err(network_error)?;
    request
        .timeout(Duration::from_secs(60))
        .map_err(network_error)?;
    request
        .post_fields_copy(body.to_string().as_bytes())
        .map_err(network_error)?;

    let mut response = Vec::new();
    let mut exceeded_limit = false;
    let result = {
        let mut transfer = request.transfer();
        transfer
            .write_function(|chunk| {
                if response.len() + chunk.len() > MAX_RESPONSE_BYTES {
                    exceeded_limit = true;
                    return Ok(0);
                }
                response.extend_from_slice(chunk);
                Ok(chunk.len())
            })
            .map_err(network_error)?;
        transfer.perform()
    };
    if exceeded_limit {
        return Err("The response exceeded the size limit. Try a narrower question.".into());
    }
    result.map_err(network_error)?;
    let status = request.response_code().map_err(network_error)?;
    if !(200..300).contains(&status) {
        // API error bodies can echo credentials and submitted text.
        return Err(match status {
            400 | 404 | 422 => format!(
                "The provider rejected the request. Check the model in {model_env} (HTTP {status})."
            ),
            401 | 403 => {
                "The provider rejected the API key. Check its permissions and billing.".into()
            }
            429 => {
                "The provider reached a usage limit. Check your quota or try again later.".into()
            }
            _ => format!("The provider could not complete the request (HTTP {status})."),
        });
    }
    let value = serde_json::from_slice(&response)
        .map_err(|_| "The provider returned an invalid response.")?;
    answer_text(provider, &value)
}

fn network_error(error: curl::Error) -> String {
    if error.is_operation_timedout() {
        "The request timed out. Try again.".into()
    } else {
        format!(
            "Could not contact the provider (network error {}).",
            error.code()
        )
    }
}

fn request_body(provider: Provider, question: &str, model: &str) -> Value {
    match provider {
        Provider::OpenAi => json!({
            "model": model, "input": question, "instructions": INSTRUCTIONS,
            "max_output_tokens": MAX_TOKENS, "store": false,
        }),
        Provider::Anthropic => json!({
            "model": model, "messages": [{"role":"user", "content":question}],
            "system": INSTRUCTIONS, "max_tokens": MAX_TOKENS,
        }),
    }
}

fn answer_text(provider: Provider, value: &Value) -> Result<String, String> {
    let mut parts = Vec::new();
    let truncated = "The answer was cut short. Try a narrower question.";
    match provider {
        Provider::OpenAi => {
            if value["status"] == "incomplete" {
                return Err(truncated.into());
            }
            if value["status"] != "completed" {
                return Err("The provider could not complete the answer.".into());
            }
            for item in value["output"].as_array().into_iter().flatten() {
                if item["type"] == "message" && item["role"] == "assistant" {
                    for part in item["content"].as_array().into_iter().flatten() {
                        match part["type"].as_str() {
                            Some("output_text") => parts.extend(part["text"].as_str()),
                            Some("refusal") => parts.extend(part["refusal"].as_str()),
                            _ => {}
                        }
                    }
                }
            }
        }
        Provider::Anthropic => {
            if value["stop_reason"] == "max_tokens" {
                return Err(truncated.into());
            }
            for part in value["content"].as_array().into_iter().flatten() {
                if part["type"] == "text" {
                    parts.extend(part["text"].as_str());
                }
            }
        }
    }
    let text = parts.join("\n").trim().to_owned();
    if text.is_empty() {
        return Err("The provider returned no text.".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_contains_only_typed_input_and_fixed_instructions() {
        let question = "Explain \"$(pwd)\"\nwithout running it";
        let openai = request_body(Provider::OpenAi, question, "example");
        assert_eq!(openai["input"], question);
        assert_eq!(openai["store"], false);
        assert!(openai.get("tools").is_none());
        let anthropic = request_body(Provider::Anthropic, question, "example");
        assert_eq!(
            anthropic["messages"],
            json!([{"role":"user","content":question}])
        );
        assert!(anthropic.get("tools").is_none());
    }

    #[test]
    fn response_filters_private_reasoning_and_collects_text() {
        let openai = json!({"status":"completed","output":[
            {"type":"reasoning","content":[{"type":"output_text","text":"private"}]},
            {"type":"message","role":"assistant","content":[
                {"type":"output_text","text":"line one"},{"type":"output_text","text":"line two"}]}
        ]});
        assert_eq!(
            answer_text(Provider::OpenAi, &openai).unwrap(),
            "line one\nline two"
        );
        let anthropic = json!({"stop_reason":"end_turn","content":[
            {"type":"thinking","thinking":"private"},{"type":"text","text":"answer"}]});
        assert_eq!(
            answer_text(Provider::Anthropic, &anthropic).unwrap(),
            "answer"
        );
    }

    #[test]
    fn incomplete_answers_never_look_like_complete_commands() {
        for (provider, response) in [
            (Provider::OpenAi, json!({"status":"incomplete"})),
            (
                Provider::Anthropic,
                json!({"stop_reason":"max_tokens","content":[{"type":"text","text":"git"}]}),
            ),
        ] {
            assert!(answer_text(provider, &response).is_err());
        }
        assert!(ask(Provider::OpenAi, "  ").is_err());
        assert!(ask(Provider::Anthropic, &"x".repeat(MAX_QUESTION_CHARS + 1)).is_err());
    }
}
