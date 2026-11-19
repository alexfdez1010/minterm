use super::{INSTRUCTIONS, MAX_TOKENS, Purpose, SUMMARY_INSTRUCTIONS, exchange, network_error};
use curl::easy::List;
use serde_json::{Value, json};

pub(super) fn chat(
    question: &str,
    purpose: Purpose,
    prefs: &crate::preferences::Preferences,
) -> Result<String, String> {
    let fetch = |path: &str, body: Option<&Value>| -> Result<Value, String> {
        let mut headers = List::new();
        headers
            .append("Content-Type: application/json")
            .map_err(network_error)?;
        let host = prefs.ollama_host.trim_end_matches('/');
        let (status, bytes) = exchange(&format!("{host}/api/{path}"), headers, body)
            .map_err(|e| format!("Check the Ollama server in Settings. {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!(
                "Ollama returned HTTP {status}. Check the server and model in Settings."
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| "Ollama returned an invalid response.".into())
    };
    let model = if prefs.ollama_model.is_empty() {
        let catalog = fetch("tags", None)?;
        catalog["models"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|model| model["name"].as_str().filter(|s| !s.is_empty()))
            .ok_or("No Ollama models installed. Pull a model and select it in Settings.")?
            .to_owned()
    } else {
        prefs.ollama_model.clone()
    };
    let (instructions, tokens) = match purpose {
        Purpose::Question => (INSTRUCTIONS, MAX_TOKENS),
        Purpose::Activity => (SUMMARY_INSTRUCTIONS, 512),
    };
    let body = json!({"model":model,"stream":false,"options":{"num_predict":tokens},
        "messages":[{"role":"system","content":instructions},{"role":"user","content":question}]});
    let response = fetch("chat", Some(&body))?;
    if response["done_reason"] == "length" {
        return Err("The answer was cut short. Try a narrower question.".into());
    }
    if response["done"] != true {
        return Err("Ollama could not complete the answer.".into());
    }
    response["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Ollama returned no text.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn selected_model_skips_discovery_and_unset_model_uses_the_catalog() {
        for (configured, model) in [("chosen:small", "chosen:small"), ("", "local:first")] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut prefs = Preferences::default();
            prefs.ollama_host = format!("http://{}", listener.local_addr().unwrap());
            prefs.ollama_model = configured.into();
            let server = thread::spawn(move || {
                let routes = if configured.is_empty() {
                    vec!["GET /api/tags", "POST /api/chat"]
                } else {
                    vec!["POST /api/chat"]
                };
                for route in routes {
                    let (mut socket, _) = listener.accept().unwrap();
                    let mut reader = BufReader::new(&socket);
                    let mut first = String::new();
                    reader.read_line(&mut first).unwrap();
                    assert!(first.starts_with(route), "{first}");
                    let mut length = 0;
                    for line in reader.by_ref().lines() {
                        let line = line.unwrap().to_lowercase();
                        if line.is_empty() {
                            break;
                        }
                        assert!(!line.starts_with("authorization:"));
                        if let Some(n) = line.strip_prefix("content-length:") {
                            length = n.trim().parse().unwrap();
                        }
                    }
                    let response = if route.starts_with("GET") {
                        json!({"models":[{"name":"local:first"}]})
                    } else {
                        let mut bytes = vec![0; length];
                        reader.read_exact(&mut bytes).unwrap();
                        let request: Value = serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(request["model"], model);
                        assert_eq!(request["stream"], false);
                        assert_eq!(request["messages"][1]["content"], "Explain this output");
                        json!({"done":true,"message":{"role":"assistant","content":"A local answer."}})
                    }.to_string();
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                }
            });
            assert_eq!(
                chat("Explain this output", Purpose::Question, &prefs).unwrap(),
                "A local answer."
            );
            server.join().unwrap();
        }
    }
}
