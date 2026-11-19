use serde_json::Value;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Step(pub &'static str, pub &'static str);

pub(super) const STARTING: Step = Step("running", "Starting agent");
const THINKING: Step = Step("running", "Thinking");
const COMPLETE: Step = Step("completed", "Run completed");
const PERMISSION: Step = Step("blocked", "Permission denied");
const AUTH: Step = Step("failed", "Sign in with the agent CLI");
const FAILED: Step = Step("failed", "The agent reported an error");

pub(super) fn diagnose(message: &str) -> Option<Step> {
    let lower = message.to_ascii_lowercase();
    let contains = |terms: &[&str]| terms.iter().any(|s| lower.contains(s));
    if contains(&[
        "not logged in",
        "login required",
        "not authenticated",
        "authentication",
        "unauthorized",
        "api key",
        "api_key",
        "run /login",
        "401 unauthorized",
    ]) {
        Some(AUTH)
    } else if contains(&[
        "not inside a trusted directory",
        "not a git repository",
        "skip-git-repo-check",
    ]) {
        Some(Step("blocked", "Open a trusted Git repository"))
    } else if contains(&[
        "permission denied",
        "permission was denied",
        "approval required",
        "approval was denied",
        "user denied",
        "permission policy",
    ]) {
        Some(PERMISSION)
    } else if contains(&[
        "rate limit",
        "quota",
        "usage limit",
        "credit balance",
        "insufficient credits",
        "429 too many requests",
    ]) {
        Some(Step("blocked", "Provider usage limit reached"))
    } else if contains(&["connection refused", "failed to connect", "network error"]) {
        Some(Step("failed", "Could not connect to the provider"))
    } else {
        None
    }
}

#[derive(Default)]
pub(super) struct Stream {
    pub end: Option<Step>,
    pub diagnostic: Option<Step>,
    last_answer: Option<u64>,
}

#[derive(Default)]
pub(super) struct Record {
    pub step: Option<Step>,
    pub text: Option<String>,
}

impl Stream {
    pub fn parse(&mut self, bytes: &[u8], claude: bool) -> Record {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            self.diagnostic = diagnose(&String::from_utf8_lossy(bytes)).or(self.diagnostic);
            return Record::default();
        };
        if claude {
            self.claude(&value)
        } else {
            self.codex(&value)
        }
    }

    fn codex(&mut self, event: &Value) -> Record {
        let kind = event["type"].as_str().unwrap_or_default();
        let item = &event["item"];
        let mut record = Record::default();
        record.step = match kind {
            "thread.started" | "turn.started" => Some(THINKING),
            "turn.completed" => {
                self.end = Some(COMPLETE);
                None
            }
            "turn.failed" | "error" => {
                let message = event["error"]["message"]
                    .as_str()
                    .or(event["message"].as_str())
                    .unwrap_or_default();
                let step = diagnose(message).or(self.diagnostic).unwrap_or(FAILED);
                self.diagnostic = Some(step);
                if kind == "turn.failed" {
                    self.end = Some(step);
                }
                Some(step)
            }
            "item.started" | "item.updated" | "item.completed" => match item["type"].as_str() {
                Some("agent_message") if kind == "item.completed" => {
                    record.text = item["text"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned);
                    Some(Step("running", "Writing a response"))
                }
                Some("command_execution") => Some(if kind == "item.completed" {
                    THINKING
                } else {
                    Step("running", "Running a command")
                }),
                Some("file_change") => Some(Step("running", "Editing files")),
                Some("mcp_tool_call" | "dynamic_tool_call") => {
                    Some(Step("running", "Using a tool"))
                }
                Some("web_search") => Some(Step("running", "Searching the web")),
                Some("todo_list") => Some(Step("running", "Updating the plan")),
                Some("reasoning") => Some(THINKING),
                Some("error") => {
                    self.diagnostic = item["message"]
                        .as_str()
                        .and_then(diagnose)
                        .or(self.diagnostic);
                    None
                }
                _ => None,
            },
            _ => None,
        };
        record
    }

    fn claude(&mut self, event: &Value) -> Record {
        let mut record = Record::default();
        match event["type"].as_str() {
            Some("system") if event["subtype"] == "init" => record.step = Some(THINKING),
            Some("assistant") => {
                let mut paragraphs = Vec::new();
                for block in event["message"]["content"].as_array().into_iter().flatten() {
                    match block["type"].as_str() {
                        Some("text") => {
                            if let Some(text) = block["text"].as_str() {
                                paragraphs.push(text);
                            }
                        }
                        Some("tool_use") => {
                            record.step = Some(Step(
                                "running",
                                match block["name"].as_str() {
                                    Some("Bash") => "Running a command",
                                    Some("Read" | "Grep" | "Glob") => "Reading files",
                                    Some("Edit" | "Write" | "MultiEdit") => "Editing files",
                                    Some("WebSearch" | "WebFetch") => "Searching the web",
                                    Some("TodoWrite" | "TaskUpdate") => "Updating the plan",
                                    Some("AskUserQuestion") => "Requested clarification",
                                    _ => "Using a tool",
                                },
                            ));
                        }
                        _ => {}
                    }
                }
                if !paragraphs.is_empty() {
                    let text = paragraphs.join("\n\n");
                    self.last_answer = Some(fingerprint(&text));
                    record.text = Some(text);
                    record
                        .step
                        .get_or_insert(Step("running", "Writing a response"));
                }
            }
            Some("result") => {
                let text = event["result"].as_str().unwrap_or_default();
                let denied = event["permission_denials"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty());
                let error = event["is_error"] == true || event["subtype"] != "success";
                self.end = Some(if error {
                    match event["subtype"].as_str() {
                        Some("error_max_turns") => Step("blocked", "Turn limit reached"),
                        Some("error_max_budget_usd") => Step("blocked", "Budget limit reached"),
                        _ if denied => PERMISSION,
                        _ => diagnose(text)
                            .or_else(|| {
                                event["errors"]
                                    .as_array()?
                                    .iter()
                                    .find_map(|v| v.as_str().and_then(diagnose))
                            })
                            .or(self.diagnostic)
                            .unwrap_or(FAILED),
                    }
                } else if denied {
                    Step("completed", "Completed with permission limits")
                } else {
                    COMPLETE
                });
                if !error && !text.is_empty() && self.last_answer != Some(fingerprint(text)) {
                    record.text = Some(text.to_owned());
                }
            }
            Some("rate_limit_event") if event["rate_limit_info"]["status"] == "rejected" => {
                self.diagnostic = Some(Step("blocked", "Provider usage limit reached"));
            }
            _ => {}
        }
        record
    }

    pub fn finish(&self, success: bool) -> Step {
        if let Some(step) = self.end
            && (success || step.0 != "completed")
        {
            return step;
        }
        self.diagnostic.unwrap_or(if success {
            Step("failed", "Agent exited without a final result")
        } else {
            FAILED
        })
    }
}

fn fingerprint(text: &str) -> u64 {
    let mut state = DefaultHasher::new();
    text.trim().hash(&mut state);
    state.finish()
}
